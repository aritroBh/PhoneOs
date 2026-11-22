import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
// Regression tests from the adversarial review of the Live view (its nine probes). The
// probes of the viewer's logic (privacy, decoding, unknowns, scene, stopped steps) moved with
// that logic to core/jarvis-viewer/tests/review.rs; what stays here is the driver's capture
// of real-screen pixels and, end to end through `LiveViewModel`, the samples' privacy.

private let phoneSize = Size(width: 393, height: 852)

private func pnode(
    _ role: Role, _ label: String? = nil, value: String? = nil, placeholder: String? = nil, _ frame: Rect,
    focused: Bool = false, secure: Bool = false, _ children: [UiNode] = []
) -> UiNode {
    UiNode(role: role, label: label, value: value, identifier: nil, placeholder: placeholder, frame: frame, enabled: true,
           visible: true, hittable: true, focused: focused, selected: false, secure: secure, children: children)
}

private func pobserved(_ root: UiNode) -> Event {
    .observed(observation: ScreenObservation(
        screen: Screen(appBundleId: "com.apple.MobileSMS", appName: "Messages", size: phoneSize, root: root, keyboardVisible: false,
                       orientation: .portrait),
        screenshot: nil, deviceLocked: false
    ))
}

private func pprogress(_ step: UInt32, _ action: Action?) -> SessionProgress {
    SessionProgress(step: step, phase: .acting, lastAction: action, app: "Messages", modelCalls: 0, parseRetries: 0,
                    waitingForUnlock: false, lockedWaitMs: 0, lockedTotalMs: 0)
}

private struct ReviewRun {
    var events: [RunEvent] = [.update(.started(TaskSpec(id: "probe", text: "Sign in")))]
    var number = 0
    var coreStep: UInt32 = 0

    mutating func observe(_ event: Event) {
        number += 1
        events.append(.update(.effect(.observe(screenshot: false), number: number)))
        events.append(.update(.effectCompleted(.observe(screenshot: false), event: event)))
    }

    mutating func act(_ command: RunnerCommand, action: Action? = nil) {
        number += 1
        coreStep += 1
        events.append(.update(.effect(.act(command: command), number: number)))
        events.append(.update(.progress(pprogress(coreStep, action))))
        events.append(.update(.effectCompleted(.act(command: command), event: .actResult(result: RunnerResult(ok: true, error: nil)))))
    }

    /// The run fed through the adapter, as the app feeds it.
    func viewer() -> LiveViewModel {
        let viewer = LiveViewModel()
        for e in events { viewer.apply(e) }
        return viewer
    }
}

/// The adapter's encoded timeline as text (the remote stream's bytes).
private func encoded(_ viewer: LiveViewModel) throws -> String {
    String(decoding: try #require(viewer.encode()), as: UTF8.self)
}

// MARK: - The driver option against a LoopCtrl stub

private func lcOK(_ result: JSONValue, delay: TimeInterval = 0) -> StubHTTPServer.Response {
    .json(["jsonrpc": "2.0", "id": 1, "result": result], delay: delay)
}

private let lcJpeg = Data([0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])

private func lcServer(screenshot: @escaping @Sendable () -> StubHTTPServer.Response) throws -> (StubHTTPServer, FakePhone) {
    let phone = FakePhone()
    let server = try StubHTTPServer { request in
        guard let body = try? request.json(), let method = body["method"]?.stringValue else {
            return .text("bad request", status: 400)
        }
        switch method {
        case "device_info":
            return lcOK(["system": ["foreground_app": "com.apple.Preferences", "screen_locked": false], "orientation": "portrait"])
        case "page_source":
            return lcOK(["format": "text", "source": .string(FakePhone.text(for: phone.page)), "keyboard": ["visible": false]])
        case "screenshot":
            return screenshot()
        case "tap":
            let params = body["params"] ?? .null
            phone.apply(.tap(x: params["x"]?.numberValue ?? -1, y: params["y"]?.numberValue ?? -1))
            return lcOK(["message": "tapped"])
        default:
            return .json(["jsonrpc": "2.0", "id": body["id"] ?? .null, "error": ["code": -32601, "message": "unknown"]])
        }
    }
    return (server, phone)
}

private func lcRun(option: Bool, observeTimeout: Duration = .seconds(10),
                   screenshot: @escaping @Sendable () -> StubHTTPServer.Response) async throws -> (RunResult, [String]) {
    let (server, _) = try lcServer(screenshot: screenshot)
    defer { server.stop() }
    var rc = LoopCtrlRunner.Configuration(baseURL: server.baseURL, requestTimeout: 5)
    rc.screenSize = Size(width: 393, height: 852)
    let runner = LoopCtrlRunner(configuration: rc)
    let h = Harness()
    let config = try SessionConfigBuilder.make(capabilities: h.model.capabilities,
                                               knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")])
    let deps = DriverDependencies(runner: runner, model: h.model, tools: h.tools, human: h.gate, memory: h.memory,
                                  screenshots: h.screenshots, viewerFrames: ViewerFrameStore())
    let driver = try SessionDriver(config: config, task: h.task, memoryJSON: "", dependencies: deps,
                                   configuration: DriverConfiguration(observeTimeout: observeTimeout, observeRetryDelay: .milliseconds(10),
                                                                      lockWatchInterval: nil, viewerScreenshots: option))
    let collector = Task { () -> [String] in
        var out: [String] = []
        for await u in driver.updates {
            if case .ioFailed(let kind, let message, _) = u { out.append("\(kind): \(message)") }
        }
        return out
    }
    let result = await driver.run()
    return (result, await collector.value)
}

@Suite("Live view review: the real-screen option must not change the run", .timeLimit(.minutes(2)), .serialized)
struct ReviewViewerDriverBehaviour {
    @Test("a runner whose screenshot call fails: the run ends the same with the option on and off")
    func failingScreenshot() async throws {
        let failing: @Sendable () -> StubHTTPServer.Response = {
            .json(["jsonrpc": "2.0", "id": 1, "error": ["code": -32000, "message": "screenshot unavailable"]])
        }
        let (off, _) = try await lcRun(option: false, screenshot: failing)
        let (on, failures) = try await lcRun(option: true, screenshot: failing)
        #expect(off.outcome.status == .success)
        #expect(on.outcome.status == off.outcome.status, "option on: \(on.outcome.status) \(on.outcome.summary); io failures: \(failures.prefix(3))")
    }

    @Test("a slow screenshot: the observation deadline is the same with the option on and off")
    func slowScreenshot() async throws {
        let slow: @Sendable () -> StubHTTPServer.Response = {
            lcOK(["format": "jpeg", "encoding": "base64", "data": .string(lcJpeg.base64EncodedString()),
                  "pixel_width": 786, "pixel_height": 1704, "point_width": 393, "point_height": 852, "scale": 2], delay: 1.2)
        }
        let (off, _) = try await lcRun(option: false, observeTimeout: .seconds(1), screenshot: slow)
        let (on, failures) = try await lcRun(option: true, observeTimeout: .seconds(1), screenshot: slow)
        #expect(off.outcome.status == .success)
        #expect(on.outcome.status == off.outcome.status, "option on: \(on.outcome.status) \(on.outcome.summary); io failures: \(failures.prefix(3))")
    }
}

// MARK: - Turning the option off mid-run

private final class PixelCountingPhone: PhoneRunner, Sendable {
    let inner: DemoPhone
    private let asked = Mutex<[Bool]>([])
    init(_ inner: DemoPhone) { self.inner = inner }
    var requests: [Bool] { asked.withLock { $0 } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        asked.withLock { $0.append(includeScreenshot) }
        var o = try await inner.snapshot(includeScreenshot: includeScreenshot)
        if includeScreenshot {
            o.screenshot = CapturedScreenshot(data: Data(repeating: 1, count: 64), mimeType: "image/jpeg", pixelWidth: 393, pixelHeight: 852)
        }
        return o
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }
    func lockState() async throws -> DeviceLockState { try await inner.lockState() }
}

@Suite("Live view review: the option turned off while a run goes", .timeLimit(.minutes(1)))
struct ReviewViewerOptionOff {
    /// `AppModel.setLiveViewScreenshots(false)` only disables the store; the driver's
    /// configuration was fixed at run start. Off should mean no screenshots are taken.
    @Test("with the store disabled (owner turned it off mid-run) the runner is not asked for pixels")
    func disabledStoreStopsCapture() async throws {
        let phone = PixelCountingPhone(DemoPhone())
        let frames = ViewerFrameStore()
        frames.isEnabled = false
        let model = ScriptedModel(steps: DemoPhone.wifiOffScript, capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: false))
        let config = try SessionConfigBuilder.make(capabilities: model.capabilities, knownApps: DemoPhone.knownApps,
                                                   adjustPolicy: { $0.firstUseApproval = false })
        let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(), memory: RecordingMemory(),
                                      screenshots: ScreenshotStore(), clock: SteppingClock(), viewerFrames: frames)
        let driver = try SessionDriver(config: config, task: TaskSpec(id: "off", text: "Turn off Wi-Fi"), memoryJSON: "", dependencies: deps,
                                       configuration: DriverConfiguration(lockWatchInterval: nil, viewerScreenshots: true))
        let drain = Task { for await _ in driver.updates {} }
        let result = await driver.run()
        await drain.value
        #expect(result.outcome.status == .success)
        #expect(frames.count == 0)
        #expect(!phone.requests.contains(true), "pixels requested after the option was turned off: \(phone.requests)")
    }
}

// MARK: - A. One privacy rule, decided by the core

/// Samples from the core's own redaction tests (jarvis-safety tests/redact.rs, review_fixes.rs).
private let otpSample = ("Your verification code is 482913.", "482913")
private let cardSample = ("Card: 5555 5555 5555 4444 exp 12/29", "5555 5555 5555 4444")
private let ibanSample = ("IBAN: GB82 WEST 1234 5698 7654 32", "GB82 WEST 1234 5698 7654 32")
private let keySample = ("key sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N here", "sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N")

private func typedField(_ label: String?, placeholder: String? = nil, identifier: String? = nil, secure: Bool = false) -> UiNode {
    let full = Rect(x: 0, y: 0, width: 393, height: 852)
    let field = UiNode(role: .textField, label: label, value: nil, identifier: identifier, placeholder: placeholder,
                       frame: Rect(x: 16, y: 200, width: 361, height: 44), enabled: true, visible: true, hittable: true,
                       focused: true, selected: false, secure: secure, children: [])
    return pnode(.application, "Shop", full, [pnode(.window, nil, full, [field])])
}


@Suite("Live view review: secrets never reach the encoded timeline through the adapter", .timeLimit(.minutes(1)))
struct ReviewViewerAdapterPrivacy {
    /// Every sample on screen (labels, a placeholder, the app name) and typed into fields,
    /// plain, credential-like and secure, fed through `LiveViewModel` as the app feeds it.
    @Test("samples shown on screen or typed never appear in encode() bytes, captions or wireframes")
    func samplesNeverEncoded() throws {
        let samples = [otpSample, cardSample, ibanSample, keySample]
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        var children: [UiNode] = []
        for (i, sample) in samples.enumerated() {
            children.append(pnode(.button, sample.0, Rect(x: 16, y: Double(100 + 60 * i), width: 361, height: 44)))
        }
        children.append(pnode(.textField, nil, placeholder: cardSample.0, Rect(x: 16, y: 500, width: 361, height: 44)))
        let tree = pnode(.application, "App", full, [pnode(.window, nil, full, children)])
        var run = ReviewRun()
        run.events.append(.update(.effect(.observe(screenshot: false), number: 1)))
        run.events.append(.update(.effectCompleted(.observe(screenshot: false), event: .observed(observation: ScreenObservation(
            screen: Screen(appBundleId: "com.example", appName: "Codes \(otpSample.0)", size: phoneSize, root: tree,
                           keyboardVisible: false, orientation: .portrait),
            screenshot: nil, deviceLocked: false
        )))))
        run.number = 1
        for i in samples.indices { run.act(.tap(x: 100, y: Double(120 + 60 * i))) }
        run.act(.tap(x: 100, y: 520))
        // Typed: into a plain field (the core's redaction would change the text), a
        // credential-like one and a secure one.
        for sample in samples {
            run.observe(pobserved(typedField("Message")))
            run.act(.typeText(text: sample.0), action: .type(element: nil, text: sample.0, submit: false))
            run.observe(pobserved(typedField("Card number")))
            run.act(.typeText(text: sample.1), action: .type(element: nil, text: sample.1, submit: false))
            run.observe(pobserved(typedField("Name", secure: true)))
            run.act(.typeText(text: sample.1), action: .type(element: nil, text: sample.1, submit: false))
        }
        let viewer = run.viewer()
        let json = try encoded(viewer)
        let t = viewer.timeline
        #expect(t.steps.count == samples.count + 1 + 3 * samples.count)
        for sample in samples {
            #expect(!json.contains(sample.1), "\(sample.1) in the encoded timeline")
            #expect(!t.steps.contains { $0.caption.contains(sample.1) })
            #expect(!t.screens.contains { ($0.appName ?? "").contains(sample.1) })
            for info in t.screens {
                let screen = try #require(viewer.screen(id: info.id))
                #expect(!screen.elements.contains { ($0.label ?? "").contains(sample.1) })
            }
        }
        #expect(t.steps[0].caption.contains("[redacted"), "what is shown is the core's redaction: \(t.steps[0].caption)")
        // Plain text in a plain field is still shown, so the checks above are not vacuous.
        var plain = ReviewRun()
        plain.observe(pobserved(typedField("Message")))
        plain.act(.typeText(text: "See you at 7"), action: .type(element: nil, text: "See you at 7", submit: false))
        let shown = plain.viewer()
        #expect(shown.timeline.steps.last?.caption == "Type \u{201C}See you at 7\u{201D}")
        #expect(try encoded(shown).contains("See you at 7"))
    }
}

// MARK: - B. Viewer pixels never change the run

private let corePixels = CapturedScreenshot(data: Data(repeating: 1, count: 64), mimeType: "image/jpeg", pixelWidth: 393, pixelHeight: 852)
private let viewerPixels = CapturedScreenshot(data: Data(repeating: 2, count: 64), mimeType: "image/jpeg", pixelWidth: 393, pixelHeight: 852)

private struct CaptureFailed: Error {}

/// The demo phone with a configurable Live view capture and lock report.
private final class ViewerPhone: PhoneRunner, Sendable {
    enum Capture: Sendable { case pixels, fails, hangs, none }

    let inner: DemoPhone
    let capture: Capture
    /// Replaces the observation's lock report when set (unlocked observations only).
    let reportedLock: DeviceLockState?
    private let state = Mutex<(asks: [Bool], captures: Int, lockNextCheck: Bool)>(([], 0, false))
    let onCapture: (@Sendable (Int) -> Void)?

    init(_ capture: Capture, inner: DemoPhone = DemoPhone(), reportedLock: DeviceLockState? = nil,
         onCapture: (@Sendable (Int) -> Void)? = nil) {
        self.inner = inner
        self.capture = capture
        self.reportedLock = reportedLock
        self.onCapture = onCapture
    }

    var asks: [Bool] { state.withLock { $0.asks } }
    var captures: Int { state.withLock { $0.captures } }
    func lockAtNextCheck() { state.withLock { $0.lockNextCheck = true } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        state.withLock { $0.asks.append(includeScreenshot) }
        var o = try await inner.snapshot(includeScreenshot: includeScreenshot)
        if let reportedLock, !o.deviceLocked { o.lockState = reportedLock }
        if includeScreenshot { o.screenshot = corePixels }
        return o
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }

    func lockState() async throws -> DeviceLockState {
        let locked = state.withLock { s -> Bool in
            defer { s.lockNextCheck = false }
            return s.lockNextCheck
        }
        return locked ? .locked : try await inner.lockState()
    }

    func viewerScreenshot() async throws -> CapturedScreenshot? {
        let n = state.withLock { s -> Int in
            s.captures += 1
            return s.captures
        }
        onCapture?(n)
        switch capture {
        case .pixels: return viewerPixels
        case .fails: throw CaptureFailed()
        case .hangs:
            try await Task.sleep(for: .seconds(30))
            return viewerPixels
        case .none: return nil
        }
    }
}

private struct ViewerRun {
    var result: RunResult
    var updates: [DriverUpdate]

    /// (effect number, the core asked pixels, locked, answered an unlock wait) per observation.
    var observations: [(number: Int, asked: Bool, locked: Bool, inUnlockWait: Bool)] {
        var out: [(Int, Bool, Bool, Bool)] = []
        var number = 0
        for u in updates {
            if case .effect(_, let n) = u { number = n }
            guard case .effectCompleted(let effect, let event) = u else { continue }
            let asked: Bool, wait: Bool
            switch effect {
            case .observe(let s): (asked, wait) = (s, false)
            case .awaitUnlock: (asked, wait) = (false, true)
            default: continue
            }
            switch event {
            case .observed(let o): out.append((number, asked, o.deviceLocked, wait))
            case .observedRunnerText(let s): out.append((number, asked, s.deviceLocked, wait))
            default: break
            }
        }
        return out
    }

    var notes: [String] { updates.compactMap { if case .note(let n) = $0 { n } else { nil } } }
    var ioFailures: [IOKind] { updates.compactMap { if case .ioFailed(let k, _, _) = $0 { k } else { nil } } }
}

private func driveViewer(_ phone: any PhoneRunner, frames: ViewerFrameStore, option: Bool = true, vision: Bool = false,
                         timeout: Duration = .seconds(1)) async throws -> ViewerRun {
    let model = ScriptedModel(steps: vision ? DemoPhone.deleteAccountScript : DemoPhone.wifiOffScript,
                              capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: vision))
    let config = try SessionConfigBuilder.make(capabilities: model.capabilities, knownApps: DemoPhone.knownApps,
                                               adjustPolicy: { $0.firstUseApproval = false; $0.maxSteps = 40 })
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(), memory: RecordingMemory(),
                                  screenshots: ScreenshotStore(), clock: SteppingClock(), viewerFrames: frames)
    let driver = try SessionDriver(
        config: config, task: TaskSpec(id: "view", text: vision ? "Delete my account" : "Turn off Wi-Fi"), memoryJSON: "",
        dependencies: deps,
        configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10), unlockPollInterval: .milliseconds(5),
                                           lockWatchInterval: nil, viewerScreenshots: option, viewerScreenshotTimeout: timeout)
    )
    let collector = Task { () -> [DriverUpdate] in
        var all: [DriverUpdate] = []
        for await u in driver.updates { all.append(u) }
        return all
    }
    let result = await driver.run()
    return ViewerRun(result: result, updates: await collector.value)
}

@Suite("Live view review: the real-screen capture is best effort and fail closed", .timeLimit(.minutes(1)), .serialized)
struct ReviewViewerCapture {
    @Test("a capture that fails: the run is the same, nothing is kept, one note per run, no IO failure")
    func failing() async throws {
        let frames = ViewerFrameStore()
        let phone = ViewerPhone(.fails)
        let run = try await driveViewer(phone, frames: frames)
        let off = try await driveViewer(ViewerPhone(.fails), frames: ViewerFrameStore(), option: false)
        #expect(run.result.outcome.status == .success && run.result.outcome.status == off.result.outcome.status)
        #expect(run.result.effectCount == off.result.effectCount, "the effect budget is not touched")
        #expect(run.observations.count >= 2)
        #expect(phone.captures == run.observations.count, "tried once per observation, never retried")
        #expect(run.notes.filter { $0.hasPrefix("Live view") }.count == 1)
        #expect(run.ioFailures.isEmpty)
        #expect(frames.count == 0)
        #expect(phone.asks == run.observations.map(\.asked))
    }

    @Test("a capture that hangs ends at its own deadline; the run goes on")
    func hanging() async throws {
        let frames = ViewerFrameStore()
        let phone = ViewerPhone(.hangs)
        let clock = ContinuousClock()
        let start = clock.now
        let run = try await driveViewer(phone, frames: frames, timeout: .milliseconds(50))
        #expect(run.result.outcome.status == .success)
        #expect(clock.now - start < .seconds(20), "each capture gave up at its deadline")
        #expect(frames.count == 0)
        #expect(run.notes.filter { $0.hasPrefix("Live view") }.count == 1)
        #expect(run.ioFailures.isEmpty)
        #expect(DriverConfiguration().viewerScreenshotTimeout <= .milliseconds(1_500))
    }

    @Test("a runner without the capability: nothing is kept and nothing is said")
    func absent() async throws {
        let frames = ViewerFrameStore()
        let phone = ViewerPhone(.none)
        let run = try await driveViewer(phone, frames: frames)
        #expect(run.result.outcome.status == .success)
        #expect(frames.count == 0 && !run.notes.contains { $0.hasPrefix("Live view") })
        let demo = try await driveViewer(DemoPhone(), frames: frames)
        #expect(demo.result.outcome.status == .success && frames.count == 0, "the protocol's default is no capture")
    }

    @Test("turned off mid-run: no further capture and nothing kept")
    func toggledOff() async throws {
        let frames = ViewerFrameStore()
        let phone = ViewerPhone(.pixels, onCapture: { n in if n == 1 { frames.isEnabled = false } })
        let run = try await driveViewer(phone, frames: frames)
        #expect(run.result.outcome.status == .success)
        #expect(run.observations.count >= 2)
        #expect(phone.captures == 1)
        #expect(frames.count == 0)
        #expect(!phone.asks.contains(true))
    }

    @Test("lock state: known unlocked gets frames; unknown gets none; a locked observation gets none")
    func lockStates() async throws {
        let known = ViewerFrameStore()
        let knownPhone = ViewerPhone(.pixels)
        let run = try await driveViewer(knownPhone, frames: known)
        #expect(Set(known.keys) == Set(run.observations.map { ViewerFrameKey(taskId: "view", observation: $0.number) }))
        #expect(run.observations.allSatisfy { known.frame(for: ViewerFrameKey(taskId: "view", observation: $0.number)) == viewerPixels })

        let unknown = ViewerFrameStore()
        let unknownPhone = ViewerPhone(.pixels, reportedLock: .unknown)
        let unknownRun = try await driveViewer(unknownPhone, frames: unknown)
        #expect(unknownRun.result.outcome.status == .success)
        #expect(unknownPhone.captures == 0 && unknown.count == 0, "a runner that cannot report its lock state gets no frames")

        let locked = ViewerFrameStore()
        let lockedPhone = ViewerPhone(.pixels, inner: DemoPhone(lockedPolls: 3))
        let lockedRun = try await driveViewer(lockedPhone, frames: locked)
        let lockedObservations = lockedRun.observations.filter(\.locked)
        #expect(!lockedObservations.isEmpty)
        #expect(lockedPhone.captures == lockedRun.observations.count - lockedObservations.count)
        for o in lockedObservations {
            #expect(locked.frame(for: ViewerFrameKey(taskId: "view", observation: o.number)) == nil)
        }
    }

    @Test("a lock arriving between the observation and the capture drops that frame")
    func lockBetween() async throws {
        let frames = ViewerFrameStore()
        let box = Mutex<ViewerPhone?>(nil)
        let phone = ViewerPhone(.pixels, onCapture: { n in if n == 1 { box.withLock { $0 }?.lockAtNextCheck() } })
        box.withLock { $0 = phone }
        let run = try await driveViewer(phone, frames: frames)
        let first = try #require(run.observations.first)
        #expect(frames.frame(for: ViewerFrameKey(taskId: "view", observation: first.number)) == nil)
        #expect(frames.count == run.observations.count - 1)
    }

    @Test("no frame for an observation taken inside an unlock wait")
    func unlockWait() async throws {
        let frames = ViewerFrameStore()
        let phone = WaitObservingPhone()
        let run = try await driveViewer(phone, frames: frames)
        #expect(run.result.outcome.status == .success, "\(run.result.outcome.summary)")
        let inWait = run.observations.filter(\.inUnlockWait)
        #expect(!inWait.isEmpty, "the wait observed (the runner could not tell its lock state)")
        for o in inWait {
            #expect(frames.frame(for: ViewerFrameKey(taskId: "view", observation: o.number)) == nil)
        }
        let kept = run.observations.filter { !$0.inUnlockWait && !$0.locked }
        #expect(!kept.isEmpty && Set(frames.keys) == Set(kept.map { ViewerFrameKey(taskId: "view", observation: $0.number) }))
        // The core answering an observation with an unlock wait drops its frame.
        #expect(!SessionDriver.viewerFrameSurvives(answer: .awaitUnlock(reason: "locked", waitedMs: 0)))
        #expect(SessionDriver.viewerFrameSurvives(answer: .observe(screenshot: false)))
    }

    @Test("pixels the core asked for are reused: no second capture")
    func coreAskedReused() async throws {
        let frames = ViewerFrameStore()
        let phone = ViewerPhone(.pixels)
        let run = try await driveViewer(phone, frames: frames, vision: true)
        #expect(run.result.outcome.status == .success)
        let asked = run.observations.filter(\.asked)
        #expect(!asked.isEmpty)
        #expect(phone.captures == run.observations.count - asked.count)
        #expect(phone.asks == run.observations.map(\.asked))
        for o in asked {
            #expect(frames.frame(for: ViewerFrameKey(taskId: "view", observation: o.number)) == corePixels)
        }
    }
}

/// Locked for its first snapshot; while locked it cannot tell its lock state (so the unlock
/// wait observes); unlocked and saying so afterwards.
private final class WaitObservingPhone: PhoneRunner, Sendable {
    let inner = DemoPhone()
    private let snapshots = Mutex(0)

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let n = snapshots.withLock { n -> Int in
            n += 1
            return n
        }
        if n == 1 {
            return RunnerObservation(
                root: pnode(.application, "SpringBoard", Rect(x: 0, y: 0, width: 393, height: 852)),
                appBundleId: "com.apple.springboard", appName: nil, screenSize: phoneSize, deviceLocked: true
            )
        }
        return try await inner.snapshot(includeScreenshot: includeScreenshot)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }

    func lockState() async throws -> DeviceLockState {
        snapshots.withLock { $0 } < 2 ? .unknown : .unlocked
    }

    func viewerScreenshot() async throws -> CapturedScreenshot? { viewerPixels }
}

// MARK: - E. Frames of unwatched runs

@Suite("Live view review: frames of unwatched runs", .timeLimit(.minutes(1)))
struct ReviewViewerStopped {
    @Test("a run's frames go when it ends with no Live view on screen; they stay while it is shown")
    func unwatched() async throws {
        let store = ViewerFrameStore()
        let key = ViewerFrameKey(taskId: "t", observation: 1)
        store.put(viewerPixels, for: key)
        store.isShown = true
        #expect(!store.runEnded() && store.count == 1)
        store.isShown = false
        #expect(store.runEnded() && store.count == 0 && store.isEnabled)

        for shown in [false, true] {
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-review-\(UUID().uuidString)")
            defer { try? FileManager.default.removeItem(at: dir) }
            let frames = ViewerFrameStore()
            frames.isShown = shown
            let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero, viewerFrames: frames)
            var settings = JarvisSettings()
            settings.liveViewScreenshots = true
            // A frame arrives while the run goes (the demo phone takes none itself).
            let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: settings, human: FakeGate()) { event in
                if case .update(.effect(.infer, _)) = event { frames.put(viewerPixels, for: key) }
            }
            guard case .finished = outcome else {
                Issue.record("not started")
                continue
            }
            #expect(frames.count == (shown ? 1 : 0), "shown: \(shown)")
        }
    }
}
