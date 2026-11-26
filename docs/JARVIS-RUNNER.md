# Jarvis's own runner

Date: 2026-11-24. Status: the protocol and both ends are built and tested on Linux. The simulator test runs both ends in one process over real sockets. **Nothing here has run on a phone.** Section 5 says what is unproven and which device spike settles each point.

## 1. Why

The core trusts a typed UI tree: roles, frames and flags. The safety gate decides on it. Today a phone has only the third-party runner's text format, which on-screen text can forge (docs/RUNNER.md 2.1), so every state-changing action on such a screen asks first. The simulator tests already use a typed runner (`XCUITestPhoneRunner`), in process. This feature gives the phone the same typed path:

- a Jarvis-owned XCUITest runner process (`JarvisRunner`) serves `XCUITestPhoneRunner`;
- the Jarvis app talks to it over loopback TCP;
- every byte is authenticated.

## 2. Parts

| Part | Where | Tested on |
|---|---|---|
| Protocol: handshake, frames, AEAD, message codec, two sans-IO state machines | `core/jarvis-runner` | Linux (Rust) |
| FFI: `JarvisRunnerClientChannel`, `JarvisRunnerServerChannel`, wire records | `core/jarvis-ffi/src/runner_channel.rs` | Linux (Rust, Swift smoke) |
| Byte-stream abstraction, in-memory pipe | `Shell/Sources/JarvisKit/Runner/JarvisRunner/RunnerByteStream.swift` | Linux |
| `RunnerServer`: any `PhoneRunner` on any stream | `.../RunnerServer.swift` | Linux |
| `JarvisRunnerClient`: a `PhoneRunner` | `.../JarvisRunnerClient.swift` | Linux |
| `RunnerKeySource`, `ProvisionedRunnerKeySource` | `.../RunnerKeySource.swift` | Linux |
| Setting `runner_kind` (`text_runner` default, `jarvis`), coordinator choice | `JarvisSettings`, `JarvisCoordinator` | Linux |
| NWListener / NWConnection streams | `Shell/App/RunnerNetwork/NWRunnerTransport.swift` | not compiled here |
| The serving UI test | `Shell/App/JarvisRunner/JarvisRunnerServeTests.swift`, target and scheme `JarvisRunner` | not compiled here |
| XCUITest runner (moved, shared by both test bundles) | `Shell/App/RunnerXCUITest/` | simulator QA |
| Simulator proof | `Shell/App/JarvisUITests/JarvisRunnerLoopbackTests.swift` | not run yet |
| App wiring (key adoption, client, Apps picker, status) | `AppModel.swift`, `AppsView.swift` | not compiled here |

## 3. Protocol (version 2)

```text
frame        = length (u32 BE, of body) || body           1 <= length <= limit
ClientHello  = 0x01 || "JRUN" || version || Nc (32)
ServerHello  = 0x02 || "JRUN" || version || Ns (32) || mac_s (32)
ClientFinish = 0x03 || mac_c (32)
Data         = 0x10 || counter (u64 BE) || ChaCha20-Poly1305 ciphertext || tag (16)

th    = "JRUN" || version || Nc || Ns
prk   = HKDF-SHA256-Extract(salt = "jarvis-runner v1" || Nc || Ns, ikm = 32-byte pre-shared key)
k_mac, k_c2s, k_s2c = Expand(prk, "mac" / "client-to-server" / "server-to-client")
mac_s = HMAC-SHA256(k_mac, "server-finished" || th)    mac_c = HMAC-SHA256(k_mac, "client-finished" || th)
nonce = direction (0x43 c→s, 0x53 s→c) || 0 0 0 || counter     aad = "JRUN" || version || 0x10 || counter
```

- **Order.** The client sends its hello. The server answers with its nonce and its proof. The client checks that proof before it sends anything else, so a server without the key never sees a request. The client then sends its proof. The server accepts no request before checking it.
- **Fresh keys per connection.** The nonces are derived by HKDF from 32 or more bytes of shell entropy per connection, so every connection gets fresh keys.
- **Strict counters.** Each direction keeps its own counter, starting at 0. A frame must carry exactly the next value. Replay, reordering, a dropped frame and a frame played back in the other direction all fail authentication, and a failure closes the channel.
- **Limits.** Limits are checked as soon as the 4 length bytes arrive, before anything is buffered:
  - before authentication: the exact handshake sizes (38 bytes to the server, 70 to the client);
  - after authentication: 256 KiB client→server and 16 MiB server→client.
  
  The body buffer grows only with bytes that actually arrive.
- **Messages.** The plaintext of a data frame is `kind || request id (u64) || not_after_ms (u64, requests only) || body`. A compact binary codec covers exactly the `PhoneRunner` surface over the `jarvis-proto` types:
  - requests: `hello(version)`, `snapshot(include_screenshot)`, `perform(RunnerCommand incl. expect)`, `lock_state`, `viewer_screenshot`, `front_bundle_id`;
  - replies: `hello(version, runner)`, `snapshot(typed tree, app, size, keyboard, orientation, locked, lock state, notes, screenshot bytes)`, `performed(RunnerResult)` (keeps `failure`: locked, stale, other), `lock_state`, `viewer_screenshot`, `front_bundle_id`, `error(message)`.
- **Trees.** Trees are written and read iteratively in pre-order: each node, then its child count. No input can exhaust a stack. Limits: 100,000 nodes, depth 256, 256 notes, 15 MiB screenshot. Numbers must be finite, strings must be UTF-8, and no bytes may follow a message.
- **Expiry (version 2).** Every request carries `not_after_ms`, under the AEAD: the last instant at which the runner may act on it, set by the client from its own deadline for that call (connecting included). The server channel is given `now_ms` with every `receive` and refuses a request received later (`RunnerError::Expired`, problem `expired`; the channel closes and nothing is surfaced). The boundary is inclusive (`now == not_after` is accepted). The server keeps the latest `now_ms` it was given, so a clock reading that goes backwards cannot make an expired request current again. The check is made on receipt; a runner call already under way is bounded by the server's own deadline, which is shorter than the client's. Version 1 peers are refused in the hello (no negotiation); a request grows by 8 bytes, every limit is unchanged.
- **The clock.** Both processes run on one phone and read the same counter: `RunnerUptimeClock.nowMs()` = `clock_gettime_nsec_np(CLOCK_MONOTONIC_RAW) / 1e6` on Apple platforms, i.e. `mach_continuous_time`, one kernel-wide count since boot that never goes backwards and keeps counting in sleep (Linux tests: `clock_gettime(CLOCK_BOOTTIME)`). Rust reads no clock; the shell passes the values in, as it does for `jarvis-remote`'s hub.
- **Deadlines** (`JarvisRunnerTiming`, derived from `DriverConfiguration()`; a test asserts the order): server 16 s < client 18 s < driver 20 s, for reads and for commands (a `wait` adds its own duration on both runner layers). The driver's deadlines are unchanged. Connecting takes at most 5 s, inside the client's deadline.
- **Idle connections.** The server closes an authenticated connection with no request for 120 s (`serverIdle`), so a connection its app abandoned cannot hold the one slot for good. An active run talks to the runner every few seconds (lock watch every 3 s, unlock polls every 2 s), and the client replaces a connection it has not used for 100 s before using it, so the server's cut never meets a request in normal use; an idle or paused app simply connects again. A connect that completes after the client gave up closes its own stream at once. If a connection was closed by the runner anyway (its idle cut, a restart), the client retries once on a fresh connection (new channel, new entropy), within the same deadline and `not_after_ms`: any request when none of its bytes was sent (the stream already knew it was closed, or refused the write with `RunnerStreamClosed`, which promises nothing left); a read (snapshot, lock state, screenshot, front app) also after it was written, since reads have no effect. A `perform` any byte of which may have left is never sent again; its failure is reported. On the phone `NWRunnerStream` does not know a peer's close before writing (TCP accepts the write), so there an idle-cut `perform` fails once and is reported; the client's own 100 s retirement makes that rare. The server's idle check runs on its injected clock (`checkIdle()`), so tests move a clock instead of waiting.
- **The stale-screen check at the runner.** `perform` carries the command's `expect`. The runner compares it with the app in front in the same step as the gesture (`XCUITestPhoneRunner`: the same main-actor hop) and answers `stale` (nothing done) when another app is in front or none can be read. A served runner that cannot check (`verifiesFrontApp` false) gets no command carrying an `expect`: `RunnerServer` answers `stale`. The driver's `FrontAppGuard` still reads the front app first; for a runner that `verifiesFrontApp` (Jarvis's client, the in-process XCUITest runner) a read that throws refuses the command instead of sending it unchecked. **No event while a notification banner is up** (Mac run 138): XCUITest holds every event it synthesizes while a banner is up (its interruption handling waits up to 15 s for the banner to go, then sends the event onto whatever is on the screen by then: a tap sent at 47.32 s was synthesized at 52.96 s, seconds after the runner's check). So the runner sends no tap, long press, swipe, typing or typed key while SpringBoard shows a banner (`BannerGate`, JarvisKit): it waits for the banner to go, at most 10 s (inside the 16 s server command deadline), and answers `stale` either way ("a notification banner was up; the gesture was not carried out"); the caller looks again. Home (a device button), launching, opening a URL and waits send no element event and are not held back. **System surfaces** (Mac run 139): Control Centre, Notification Centre and the App Switcher pulled over an app leave the app in the foreground; the runner reads them from SpringBoard's tree (`cc-root-folder-view`, `SBCoverSheetWindow` without the lock screen's signature, `handoff-banner`; `SystemSurface`, JarvisKit) and reports the top layer as `unknown.front.app` with a note naming the surface, nothing of it read; a command planned on the app is `stale` while one is up, one planned on a surface is `stale` once it has gone or another is up; the banner gate does not wait under a surface (Notification Centre's rows carry the banner's identifier); Home dismisses all three. One existence query on SpringBoard covers an alert, a banner and the surfaces (SpringBoard's tree only when it says yes): per look 2 → 1 queries, per gesture 3 → 2. **Open window:** a banner or a system alert that comes up between the runner's last check and XCUITest's own event cannot be kept out from XCUITest; a call that took longer than a held event explains (4 s plus the command's own duration) is said in the next observation's notes ("the gesture was held N s by the system; the screen may have changed before it landed"); the result itself stays as XCUITest gave it.
- **Typed only.** This protocol has no text-tree form.
- **One request at a time.** A reply must carry the outstanding request's id and answer its kind, or be an error.
- **Replies that cannot be sent.** A reply the server cannot encode, or one that is too large, is replaced by an `error` reply, so the client never waits for nothing.
- **Primitives.** HKDF-SHA256, HMAC-SHA256 (constant-time verify) and ChaCha20-Poly1305 from the RustCrypto crates `jarvis-remote` already uses, at the same versions. No new dependency. Keys are zeroized on drop. Rust has no randomness of its own; the shell passes entropy, as it does for remote control.

## 4. Threat model of the loopback channel

- **Assets.** The typed tree the safety gate trusts, and the commands that act on the phone. Commands include typed text, which may hold profile values.
- **Attacker.** Any app installed on the phone. It can connect to 127.0.0.1 ports and bind a free port first, and it can read nothing of either process.
- **(a) A stranger sends commands to the runner.** It cannot produce `mac_c`, so the server never delivers a request. It also cannot replay a recorded session: the server's fresh `Ns` changes every key.
  - Tests: `a_client_with_the_wrong_key_gets_nothing`, `a_recorded_session_replayed_into_a_new_connection_fails` (Rust); `wrongKeyClient`, `wholeSessionReplay` (Swift).
- **(b) A stranger squats the port and serves forged "typed" trees.** It cannot produce `mac_s` over the client's fresh `Nc`, and it cannot replay an old server hello. The client sends no request, accepts no reply and reports `JarvisRunnerError.unauthenticated`. The driver treats that like an unreadable screen; it never becomes an observation, and the app never falls back to the text runner.
  - Tests: `a_squatting_server_is_never_trusted` (Rust); `squatter` (Swift); step 4 of the simulator test.
- **Tampering.** Changing any byte of the handshake or of a frame is detected.
  - Tests: `handshake_tampering_is_detected`, `bit_flips_in_a_real_session_are_caught`.
- **Garbage.** Random bytes in random splits never panic, take bounded time and never yield a message.
  - Tests: `random_bytes_never_panic_and_never_yield_a_message` (Rust); `randomBytes` (Swift).
- **Not defended.**
  - **Denial of service.** A stranger can hold the single connection slot until the 5 s handshake deadline drops it, reconnect, or squat the port; Jarvis then reports the runner unreachable or unauthenticated. Even a stranger cycling the slot between Jarvis's front-app read and its command cannot make an unchecked command: the read failing refuses the command, and the runner checks `expect` next to the gesture.
  - **The fixed port reveals that a runner is present** (accepted, twenty-first review LOW-4): any app can see that something listens on 127.0.0.1:47831. It learns nothing more: it cannot complete the handshake.
  - **A stolen key.** Anyone who reads the key (a jailbroken phone, a debugger) can do what the app can.
  - **A compromised phone.**
  - **Traffic analysis.** Frame sizes are visible to nothing on loopback but the two processes and the kernel.
- **Accepted design limits.**
  - There is no forward secrecy beyond the per-connection keys (no DH): a leaked key decrypts recorded loopback traffic. Recording loopback traffic needs privileges another app does not have.
  - The key lives in the app's Keychain (this device only, after first unlock) and in the runner's memory. It is never logged and never put in a URL, a file, an error or `Debug` output.

## 4a. Twenty-first review (adversarial, 2026-11-24)

Result: no HIGH, one MED, four LOW; all fixed or accepted here, the reviewer's 19 probes kept as regression tests (`core/jarvis-runner/tests/review.rs`, `core/jarvis-ffi/tests/runner_review.rs`; the version probe now also refuses version 1).

- **MED-1, the stale-screen check failed open over this runner.** A failed front-app read counted as "cannot tell" and the command went out unchecked, and the client sent no `expect`. Fixed: `expect` travels with the command and the runner checks it next to the gesture; for a runner that `verifiesFrontApp` a failed read refuses the command.
- **LOW-1, a leaked authenticated connection.** A connect completing after the client's deadline was never closed and held the only slot. Fixed: it closes itself; the server also closes idle authenticated connections (120 s).
- **LOW-2, inverted timeout layering** (driver 20 s inside client 35/50 s inside server 30/45 s). Fixed: server 16 s < client 18 s < driver 20 s, from one place.
- **LOW-3, requests had no expiry.** Fixed: `not_after_ms` (protocol version 2).
- **LOW-4, the fixed port reveals a runner.** Accepted (section 4).

The fixed port is also what keeps a relay out today: an app that bound the runner's port could only forward bytes it cannot read, and no other port carries Jarvis's traffic. A port chosen by the runner (and passed with the key) would let a stranger sit between the two processes and hold requests back; that must not land without the request expiry, which now exists.

Unverified items the review listed (added as spikes R6 and R7 below):

- whether `NWListener` with `requiredLocalEndpoint` = 127.0.0.1 binds loopback only on a device;
- whether XCTest records `launchEnvironment`, and so the runner key, in Mac-side logs or the result bundle.

## 5. What is not proven, and the device spikes

Proven here:

- Rust: 33 new tests, plus 19 review probes and 7 expiry tests (twenty-first review).
- Shell, Swift Testing on Linux: 15 new tests over in-memory pipes with the real core channels, plus 11 for the twenty-first review (runner-side stale check, timeouts, leaked connections, idle timeout, expiry).
- `cargo check` for `aarch64-apple-ios`.
- The Swift smoke test of the bindings.

Not proven:

- **Not compiled.** The Mac-only files: `NWRunnerTransport.swift`, `JarvisRunnerServeTests.swift`, `JarvisRunnerLoopbackTests.swift`, the `AppModel` and `AppsView` changes and the new XcodeGen target. They follow the patterns of `RemoteAdapters.swift` and the existing UI tests. A CI run (`scripts/sim-qa.sh`) is the first compile.
- **Not run.** `JarvisRunnerLoopbackTests` on the simulator.
- **Not run on a phone.** Nothing.

Device spikes, numbered R1 to R5 so they do not collide with PLAN.md's S1 to S8:

- **R1: starting and keeping the runner alive without a Mac.** A UI-test bundle is normally started by Xcode (`xcodebuild test`) and lives while that test runs. The open questions:
  - Does `JarvisRunner-Runner.app` start from its icon on iOS 27 with a free Apple ID? It needs a mounted Developer Disk Image (LoopCtrl mounts it in-app; iCTRL uses StikDebug), and possibly a loopback VPN.
  - Does `testServeUntilStopped` run when started that way?
  - Does the scheme's `JARVIS_RUNNER_SERVE=1` reach it? If not, the skip condition must change.
  - How long does it live? LoopCtrl adds `SilentAudioKeepAlive` (not done here).
  - What survives a reboot (the DDI must be mounted again)?
  - Measure each. Record whether the 3-app free-ID limit holds with Jarvis, `JarvisRunner-Runner` and SideStore.
- **R2: key provisioning.** The candidate implemented here works like this:
  - The runner makes a fresh key on each start and launches Jarvis (`XCUIApplication().launch()`) with `JARVIS_RUNNER_KEY` (base64) in the launch environment.
  - The app adopts the key into its Keychain at launch (`ProvisionedRunnerKeySource`).
  
  To check:
  - (a) the launch environment reaches the app on a device;
  - (b) no other app can launch Jarvis with an environment (expected: only XCTest and debuggers can);
  - (c) whether the variable shows up anywhere persistent (sysdiagnose, crash logs).
  
  Alternatives, if (a) fails or (c) is bad:
  - a shared Keychain access group, or an App Group, between the app and `JarvisRunner-Runner` with a free Apple ID (both need entitlements; untested);
  - the app generates the key and the runner reads it from the app's UI. This was rejected: other UI readers could see it.
- **R3: does the runner survive the lock?** Things to measure with the screen off, and with a passcode lock:
  - whether the listener keeps accepting;
  - whether the connection survives;
  - what `XCUITestPhoneRunner`'s lock judge reports.
  
  The protocol passes the lock state through. The driver's lock handling is unchanged.
- **R4: port choice and collision.** The fixed port is `JarvisRunnerDefaults.port` = 47831. To check:
  - whether another app can bind it first (expected: yes; the app then reports `unauthenticated` or unreachable, the safe outcome);
  - whether the listener can rebind after a crash;
  - whether a port collision with LoopCtrl (8300) or iCTRL (8200) is possible (no: different ports).
  
  A squatter blocks the Jarvis runner but cannot fool it. Making that recoverable (a port list, or a port chosen by the runner and passed with the key) is follow-up work, and needs the request expiry (section 4a), which now exists.
- **R6: loopback-only listener (device).** Check with a second device on the same Wi-Fi (and over USB networking) that nothing can connect to the runner's port from outside the phone, i.e. that `requiredLocalEndpoint` = `127.0.0.1` binds loopback only. The listener's `isLoopback` check stays as a second line either way.
- **R7: the key in Mac-side records (Mac).** Run the `JarvisRunner` scheme from Xcode and from `xcodebuild test -resultBundlePath`, then search the result bundle, `~/Library/Developer/Xcode/DerivedData/*/Logs` and the console log for the base64 key. If XCTest records `launchEnvironment`, provisioning needs another path (R2's alternatives) or the key must be rotated after every Mac-started run.
- **R5: simulator and CI.** Things to confirm:
  - `JarvisRunnerLoopbackTests` passes in `sim-qa.sh`;
  - the Jarvis scheme compiles `JarvisRunner` without running it (it is in the build list with `[test]` and absent from the test action);
  - `JarvisRunnerServeTests` skips if it is ever run without the variable.

## 6. Using it

1. Run the `JarvisRunner` scheme's test on the phone (with a Mac today; without one after R1). It relaunches Jarvis with a fresh key and serves on 127.0.0.1:47831.
2. In Jarvis: Apps → Endpoints → Runner → **Jarvis runner**. The status line says whether the runner answered and proved its key.

If no key is set up, a run with the Jarvis runner does not start, and the app says why. It never falls back to the text runner.
