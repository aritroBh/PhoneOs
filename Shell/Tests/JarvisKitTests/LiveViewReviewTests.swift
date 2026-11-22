import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

// Regression tests from the adversarial review of the Live view (its nine probes, kept as
// written). Each test states the safe / expected behaviour.

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

    func timeline() -> ViewerTimeline {
        var t = PointerTimeline()
        for e in events { t.apply(e) }
        return t.timeline
    }
}

private func encoded(_ t: ViewerTimeline) throws -> String {
    String(decoding: try JSONEncoder().encode(t), as: UTF8.self)
}

@Suite("Live view review: Live view privacy vs the core's redaction", .timeLimit(.minutes(1)))
struct ReviewViewerPrivacy {
    /// A message bubble with a one-time code and a saved card shown on screen. The core's
    /// `redact_element` masks both before any prompt; the viewer should not be weaker.
    @Test("on-screen OTPs and card numbers do not reach wireframe labels, captions or the encoded timeline")
    func screenTextIsRedacted() throws {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        let otpFrame = Rect(x: 16, y: 300, width: 361, height: 60)
        let tree = pnode(.application, "Messages", full, [pnode(.window, nil, full, [
            pnode(.staticText, "Your Apple ID code is 482913. Do not share it.", otpFrame),
            pnode(.cell, "Visa 4111 1111 1111 1111", Rect(x: 16, y: 400, width: 361, height: 44)),
        ])])
        var run = ReviewRun()
        run.observe(pobserved(tree))
        run.act(.tap(x: 100, y: 320))
        let t = run.timeline()
        let json = try encoded(t)
        // Bound first: the Swift Testing macro (6.4.3) fails `!(x?.y ?? "").contains(…)` whatever
        // the value (it evaluates the negation to `()`); the assertion is the same.
        let caption = t.steps.last?.caption ?? ""
        #expect(!caption.contains("482913"), "caption: \(caption)")
        #expect(!json.contains("482913"), "the OTP is in the encoded timeline")
        #expect(!json.contains("4111 1111 1111 1111"), "the card number is in the encoded timeline")
    }

    /// The model types an OTP (e.g. from the owner's answer or the task text) into a plain
    /// "Verification code" field. The core treats that field as credential-like and never
    /// shows its value; LiveRun shows "Type into …" only.
    @Test("an OTP typed into a plain 'Verification code' field is not shown")
    func typedOTPHidden() throws {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        let tree = pnode(.application, "Bank", full, [pnode(.window, nil, full, [
            pnode(.textField, "Verification code", Rect(x: 16, y: 200, width: 361, height: 44), focused: true),
        ])])
        var run = ReviewRun()
        run.observe(pobserved(tree))
        run.act(.typeText(text: "482913"), action: .type(element: nil, text: "482913", submit: false))
        let t = run.timeline()
        #expect(!(try encoded(t)).contains("482913"), "caption: \(t.steps.last?.caption ?? "")")
    }

    @Test("a card number typed into a plain 'Card number' field is not shown")
    func typedCardHidden() throws {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        let tree = pnode(.application, "Shop", full, [pnode(.window, nil, full, [
            pnode(.textField, "Card number", Rect(x: 16, y: 200, width: 361, height: 44), focused: true),
        ])])
        var run = ReviewRun()
        run.observe(pobserved(tree))
        run.act(.typeText(text: "4111 1111 1111 1111"), action: .type(element: nil, text: "4111 1111 1111 1111", submit: false))
        let t = run.timeline()
        #expect(!(try encoded(t)).contains("4111"), "caption: \(t.steps.last?.caption ?? "")")
    }
}

@Suite("Live view review: secure-flagged field on a runner-text screen", .timeLimit(.minutes(1)))
struct ReviewViewerSecureFlagText {
    /// The core renders a secure-flagged TextField (UiNode.secure, role TextField) as
    /// `TextField` in the prompt (render.rs role_name + flags: no secure marker). On a
    /// runner-text screen that prompt line is the viewer's only evidence.
    @Test("typing into a secure-flagged TextField named by the prompt on a text screen is not shown")
    func secureFlaggedTextField() throws {
        let prompt = """
        <<<SCREEN_DATA>>>
        App: Bank (com.example.bank)
        [1] TextField "Passcode" (focused)
        <<<END_SCREEN_DATA>>>
        """
        var run = ReviewRun()
        run.observe(.observedRunnerText(snapshot: RunnerSnapshot(
            source: "x", appBundleId: "com.example.bank", appName: "Bank", size: phoneSize, keyboardVisible: true,
            orientation: .portrait, screenshot: nil, deviceLocked: false
        )))
        run.events.append(.prompt(prompt))
        run.act(.typeText(text: "731904"), action: .type(element: 1, text: "731904", submit: false))
        let t = run.timeline()
        #expect(!(try encoded(t)).contains("731904"), "caption: \(t.steps.last?.caption ?? "")")
    }
}

@Suite("Live view review: Live view forward-compatible decoding", .timeLimit(.minutes(1)))
struct ReviewViewerDecoding {
    private func sampleJSON() throws -> String {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        let tree = pnode(.application, "Mail", full, [pnode(.window, nil, full, [
            pnode(.button, "Send", Rect(x: 16, y: 400, width: 361, height: 44)),
        ])])
        var run = ReviewRun()
        run.observe(pobserved(tree))
        run.act(.tap(x: 100, y: 420))
        run.act(.swipe(from: Point(x: 200, y: 700), to: Point(x: 200, y: 200), durationMs: 300))
        return try encoded(run.timeline())
    }

    @Test("an unknown gesture kind from a newer sender does not fail the whole timeline")
    func unknownGesture() throws {
        let json = try sampleJSON().replacingOccurrences(of: "\"kind\":\"swipe\"", with: "\"kind\":\"pinch\"")
        #expect(json.contains("pinch"))
        let decoded = try? JSONDecoder().decode(ViewerTimeline.self, from: Data(json.utf8))
        #expect(decoded != nil, "one unknown gesture made the whole timeline undecodable")
    }

    @Test("an unknown screen source / step state / activity does not fail the whole timeline")
    func unknownOthers() throws {
        let base = try sampleJSON()
        for (old, new) in [("\"source\":\"tree\"", "\"source\":\"mirror\""),
                           ("\"kind\":\"done\"", "\"kind\":\"skipped\""),
                           ("\"kind\":\"idle\"", "\"kind\":\"paused\""),
                           ("\"kind\":\"acting\"", "\"kind\":\"paused\"")] where base.contains(old) {
            let json = base.replacingOccurrences(of: old, with: new)
            let decoded = try? JSONDecoder().decode(ViewerTimeline.self, from: Data(json.utf8))
            #expect(decoded != nil, "\(new) made the whole timeline undecodable")
        }
    }
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

private func typeStep(into root: UiNode, _ text: String) throws -> (ViewerStep, String) {
    var run = ReviewRun()
    run.observe(pobserved(root))
    run.act(.typeText(text: text), action: .type(element: nil, text: text, submit: false))
    let t = run.timeline()
    return (try #require(t.steps.last), try encoded(t))
}

@Suite("Live view review: screen text goes through the core's redaction", .timeLimit(.minutes(1)))
struct ReviewViewerRedaction {
    @Test("labels, placeholders, captions and the app name hold no OTP, card number, IBAN or key")
    func labelsAndCaptions() throws {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        let samples = [otpSample, cardSample, ibanSample, keySample]
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
        let t = run.timeline()
        let json = try encoded(t)
        let screen = try #require(t.currentScreen)
        for sample in samples {
            #expect(!json.contains(sample.1), "\(sample.1) in the encoded timeline")
            #expect(!screen.elements.contains { ($0.label ?? "").contains(sample.1) })
            #expect(!t.steps.contains { $0.caption.contains(sample.1) })
        }
        // What is shown is the core's redaction of the label, capped.
        for (i, sample) in samples.enumerated() {
            let expected = try #require(ViewerPrivacy.label(sample.0))
            #expect(expected == ViewerPrivacy.capped(try redactScreenText(text: sample.0), ViewerPrivacy.maxLabelCharacters))
            #expect(t.steps[i].caption == "Tap \u{201C}\(expected)\u{201D}")
            #expect(expected.contains("[redacted"))
        }
        #expect(screen.elements.last?.label?.contains("[redacted card]") == true, "the placeholder is redacted too")
        #expect(screen.appName?.contains("482913") == false)
    }

    @Test("a secret is redacted before the label is cut, so a cut cannot split it out of the detector")
    func redactBeforeCap() throws {
        let long = String(repeating: "x", count: 30) + " card 4111 1111 1111 1111"
        let label = try #require(ViewerPrivacy.label(long))
        #expect(!label.contains("4111"), "\(label)")
        #expect(ViewerPrivacy.label("Send") == "Send")
        #expect(ViewerPrivacy.label("") == nil && ViewerPrivacy.label(nil) == nil)
    }

    @Test("links: the host of http and https, only the scheme otherwise")
    func links() {
        #expect(ViewerPrivacy.link(of: "https://user:pw@Example.com:8443/path?q=1") == "example.com")
        #expect(ViewerPrivacy.link(of: "http://example.org") == "example.org")
        #expect(ViewerPrivacy.link(of: "myapp://SECRETTOKEN123/reset?code=1") == "myapp:")
        #expect(ViewerPrivacy.host(of: "myapp://SECRETTOKEN123/reset") == nil, "a custom scheme's host can be a token")
        #expect(ViewerPrivacy.link(of: "tel:+15551234567") == "tel:")
        #expect(ViewerPrivacy.link(of: "notaurl") == nil)
        #expect(ViewerPrivacy.link(of: "1bad:thing") == nil)
        var run = ReviewRun()
        run.act(.openUrl(url: "myapp://SECRETTOKEN123/reset"))
        run.act(.openUrl(url: "https://Example.com/a"))
        let t = run.timeline()
        #expect(t.steps.map(\.caption) == ["Open a myapp: link", "Open example.com"])
        #expect(t.steps[0].gesture == .openURL(host: "myapp:"))
        #expect(!((try? encoded(t)) ?? "SECRETTOKEN").contains("SECRETTOKEN"))
    }
}

@Suite("Live view review: typed text and credential-like fields", .timeLimit(.minutes(1)))
struct ReviewViewerTyping {
    @Test("a field the core calls credential-like, by label, placeholder or identifier, never shows typed text")
    func credentialLikeFields() throws {
        let cases: [(UiNode, String)] = [
            (typedField("CVV"), "CVV"),
            (typedField("Card number"), "Card number"),
            (typedField("IBAN"), "IBAN"),
            (typedField("One-time code"), "One-time code"),
            (typedField(nil, placeholder: "Verification code"), "Verification code"),
            (typedField(nil, placeholder: "Card number"), "Card number"),
            (typedField("Name", secure: true), "Name"),
        ]
        for (tree, label) in cases {
            let (step, json) = try typeStep(into: tree, "731904")
            #expect(step.caption == "Type into \u{201C}\(label)\u{201D}", "\(label)")
            #expect(step.gesture == .type(text: nil))
            #expect(!json.contains("731904"), "\(label)")
        }
        // An identifier the core would use as the label.
        let byIdentifier = typedField(nil, identifier: "Verification code")
        let (step, json) = try typeStep(into: byIdentifier, "731904")
        #expect(step.caption == "Type text" && !json.contains("731904"))
        // The core's answer is what decides.
        #expect(ViewerPrivacy.fieldValueHidden(role: .textField, label: "Card number", placeholder: nil, identifier: nil, secure: false))
        #expect(ViewerPrivacy.fieldValueHidden(role: .textField, label: nil, placeholder: "CVV", identifier: nil, secure: false))
        #expect(!ViewerPrivacy.fieldValueHidden(role: .textField, label: "Email", placeholder: nil, identifier: nil, secure: false))
    }

    @Test("text the core's redaction would change is not shown, even in a plain field")
    func redactedText() throws {
        for sample in [otpSample, cardSample, ibanSample, keySample] {
            let (step, json) = try typeStep(into: typedField("Message"), sample.0)
            #expect(step.caption == "Type into \u{201C}Message\u{201D}", "\(sample.0)")
            #expect(!json.contains(sample.1))
        }
        let (plain, _) = try typeStep(into: typedField("Message"), "See you at 7")
        #expect(plain.caption == "Type \u{201C}See you at 7\u{201D}", "plain text in a plain field is still shown")
    }

    @Test("on a runner-text screen typed text is never shown; the label comes from the prompt, redacted")
    func runnerTextScreens() throws {
        let prompt = """
        <<<SCREEN_DATA>>>
        App: Notes (com.apple.mobilenotes)
        [1] TextField "Title"
        [2] TextView "Code 482913 is your login code" (focused)
        <<<END_SCREEN_DATA>>>
        """
        for element: UInt32? in [1, 2, nil] {
            var run = ReviewRun()
            run.observe(.observedRunnerText(snapshot: RunnerSnapshot(
                source: "x", appBundleId: "com.apple.mobilenotes", appName: "Notes", size: phoneSize, keyboardVisible: true,
                orientation: .portrait, screenshot: nil, deviceLocked: false
            )))
            run.events.append(.prompt(prompt))
            run.act(.typeText(text: "Groceries"), action: .type(element: element, text: "Groceries", submit: false))
            let t = run.timeline()
            let step = try #require(t.steps.last)
            let json = try encoded(t)
            #expect(step.gesture == .type(text: nil))
            #expect(!json.contains("Groceries") && !json.contains("482913"), "\(String(describing: element))")
            if element == 1 { #expect(step.caption == "Type into \u{201C}Title\u{201D}") }
        }
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

// MARK: - C. Decoding from a newer sender

@Suite("Live view review: unknown values decode, render neutrally and round-trip", .timeLimit(.minutes(1)))
struct ReviewViewerUnknowns {
    @Test("unknown gesture, step state, activity and source keep their tags through a round trip")
    func roundTrip() throws {
        let screen = ViewerScreen(id: 1, size: ViewerSize(width: 393, height: 852), appName: nil, bundleId: "b", source: .unknown("mirror"),
                                  elements: [], elementsTruncated: false, frameKey: nil, focusedInputSecure: nil)
        let step = ViewerStep(number: 1, coreStep: nil, screenID: 1, gesture: .unknown("pinch"), target: nil, caption: "Pinch",
                              state: .unknown("skipped"))
        let stopped = ViewerStep(number: 2, coreStep: nil, screenID: 1, gesture: .wait(ms: 5), target: nil, caption: "Wait",
                                 state: .stopped)
        let t = ViewerTimeline(taskId: "x", steps: [step, stopped], screens: [screen], currentScreenID: 1, activity: .unknown("paused"))
        let json = try encoded(t)
        for tag in [#""kind":"pinch""#, #""kind":"skipped""#, #""kind":"paused""#, #""source":"mirror""#, #""kind":"stopped""#] {
            #expect(json.contains(tag), "\(tag)")
        }
        let back = try JSONDecoder().decode(ViewerTimeline.self, from: Data(json.utf8))
        #expect(back == t)
        let sorted = JSONEncoder()
        sorted.outputFormatting = [.sortedKeys]
        #expect(try sorted.encode(back) == (try sorted.encode(t)), "encoding is stable")
        // Known values keep their tags.
        for source: ViewerScreen.Source in [.tree, .runnerText, .locked] {
            #expect(try JSONDecoder().decode(ViewerScreen.Source.self, from: try JSONEncoder().encode(source)) == source)
        }
        #expect(ViewerScreen.Source.runnerText.tag == "runner_text")
        // An outcome this reader does not know.
        let finished = try JSONDecoder().decode(ViewerActivity.self, from: Data(#"{"kind":"finished","outcome":"exploded"}"#.utf8))
        #expect(finished == .unknown("finished"))
    }

    @Test("unknowns render neutrally: no cursor movement, not failed, a neutral caption")
    func rendering() {
        let g = ViewerGesture.unknown("pinch")
        #expect(!g.isPointer && g.kind == "pinch")
        let rest = ViewerPoint(x: 10, y: 20)
        let pose = CursorPath.pose(g, from: rest, screen: .fallbackPhone, atMs: 100)
        #expect(pose.position == rest && pose.phase == .parked)
        #expect(!ViewerStepState.unknown("x").isFailed && ViewerStepState.unknown("x").title == nil)
        #expect(ViewerActivity.unknown("paused").caption == "Working…" && ViewerActivity.unknown("paused").isIdle)
    }
}

// MARK: - D. Pure pieces the SwiftUI view reads

private func tapsTimeline(_ points: [(Double, Double)], finished: Bool = false) -> ViewerTimeline {
    let full = Rect(x: 0, y: 0, width: 393, height: 852)
    var run = ReviewRun()
    run.observe(pobserved(pnode(.application, "App", full, [])))
    for p in points { run.act(.tap(x: p.0, y: p.1)) }
    var pt = PointerTimeline()
    for e in run.events { pt.apply(e) }
    pt.apply(.update(.effect(.infer(request: ModelRequest(system: "", user: "", image: nil, marks: [], maxTokens: 1, grammar: nil,
                                                           jsonSchema: nil, kind: .text)), number: 99)))
    if finished {
        pt.apply(.update(.finished(RunResult(outcome: Outcome(status: .success, summary: "", steps: 1, durationMs: 1),
                                             endReason: .sessionFinished, memory: .saved, effectCount: 1, modelCalls: 1))))
    }
    return pt.timeline
}

@Suite("Live view review: frames, fitting and continuity", .timeLimit(.minutes(1)))
struct ReviewViewerScene {
    @Test("needs frames: replay playing, a step animation in flight, the idle pulse of a running run; otherwise static")
    func needsFrames() {
        let running = tapsTimeline([(100, 100), (300, 700)])
        var live = ReplayState()
        live.sync(numbers: running.stepNumbers, now: 0)
        // The newest step just started: in flight.
        #expect(LiveViewScene.needsFrames(timeline: running, replay: live, now: 0.01))
        // Played out, Jarvis thinking: the idle pulse needs frames, but not under Reduce Motion.
        #expect(LiveViewScene.needsFrames(timeline: running, replay: live, now: 100))
        #expect(!LiveViewScene.needsFrames(timeline: running, replay: live, now: 100, reduceMotion: true))

        let finished = tapsTimeline([(100, 100), (300, 700)], finished: true)
        var replay = ReplayState()
        replay.sync(numbers: finished.stepNumbers, now: 0)
        #expect(!LiveViewScene.needsFrames(timeline: finished, replay: replay, now: 100), "a finished run, following, settled: static")
        replay.stepBack(numbers: finished.stepNumbers, now: 200)
        #expect(LiveViewScene.needsFrames(timeline: finished, replay: replay, now: 200.05), "the scrubbed step animates")
        #expect(!LiveViewScene.needsFrames(timeline: finished, replay: replay, now: 300), "paused replay of a finished run: static")
        replay.togglePlay(numbers: finished.stepNumbers, now: 300)
        #expect(LiveViewScene.needsFrames(timeline: finished, replay: replay, now: 10_000), "playing")
        #expect(!LiveViewScene.needsFrames(timeline: ViewerTimeline(), replay: ReplayState(), now: 5), "no run: static")
    }

    @Test("the screenshot is fitted into the screen rect keeping its aspect ratio")
    func aspectFit() {
        let rect = ViewerRect(x: 10, y: 20, width: 200, height: 400)
        #expect(rect.aspectFit(ViewerSize(width: 393, height: 786)) == ViewerRect(x: 10, y: 20, width: 200, height: 400))
        let wide = rect.aspectFit(ViewerSize(width: 400, height: 200))
        #expect(wide == ViewerRect(x: 10, y: 170, width: 200, height: 100))
        let tall = rect.aspectFit(ViewerSize(width: 100, height: 400))
        #expect(tall == ViewerRect(x: 60, y: 20, width: 100, height: 400))
        #expect(rect.aspectFit(ViewerSize(width: 0, height: 10)) == rect)
        #expect(rect.aspectFit(ViewerSize(width: .nan, height: 10)) == rect)
        #expect(ViewerRect.zero.aspectFit(ViewerSize(width: 1, height: 1)) == .zero)
        let m = ViewerMapping(screen: ViewerSize(width: 393, height: 852), view: ViewerSize(width: 300, height: 600))
        let fitted = m.contentRect.aspectFit(ViewerSize(width: 786, height: 1704))
        #expect(abs(fitted.width - m.contentRect.width) < 1e-9 && abs(fitted.height - m.contentRect.height) < 1e-9)
    }

    @Test("following live, a step arriving mid-glide starts from where the cursor is: no jump")
    func continuity() throws {
        let one = tapsTimeline([(20, 20)])
        var replay = ReplayState()
        replay = LiveViewScene.synced(replay, timeline: one, now: 0)
        // 0.1 s into the first glide (it starts at the centre).
        let mid = LiveViewScene.make(timeline: one, replay: replay, now: 0.1).pose.position
        #expect(mid != ViewerPoint(x: 20, y: 20) && mid != ViewerSize.fallbackPhone.center)
        let two = tapsTimeline([(20, 20), (380, 800)])
        replay = LiveViewScene.synced(replay, timeline: two, now: 0.1)
        #expect(replay.current == 2)
        let start = LiveViewScene.make(timeline: two, replay: replay, now: 0.1).pose.position
        #expect(start.distance(to: mid) < 1e-6, "the new step starts where the cursor was (\(mid)), not at \(start)")
        let end = LiveViewScene.make(timeline: two, replay: replay, now: 0.1 + CursorPath.playMs(step: 2, in: two, from: replay.origin) / 1_000)
        #expect(end.pose.position == ViewerPoint(x: 380, y: 800) || end.step == nil)
        // A step arriving after the previous one played out starts from its rest point.
        var settled = ReplayState()
        settled = LiveViewScene.synced(settled, timeline: one, now: 0)
        settled = LiveViewScene.synced(settled, timeline: two, now: 50)
        #expect(settled.origin == ViewerPoint(x: 20, y: 20))
        let first = LiveViewScene.make(timeline: two, replay: settled, now: 50).pose.position
        #expect(first == ViewerPoint(x: 20, y: 20))
        // Replay never carries an origin.
        settled.stepBack(numbers: two.stepNumbers, now: 60)
        #expect(settled.origin == nil)
    }
}

// MARK: - E. Stopped steps and unwatched runs

@Suite("Live view review: stopped steps and frames of unwatched runs", .timeLimit(.minutes(1)))
struct ReviewViewerStopped {
    @Test("a step cancelled mid-command reads Stopped, claiming nothing about the phone")
    func stopped() {
        var run = ReviewRun()
        run.observe(pobserved(pnode(.application, "App", Rect(x: 0, y: 0, width: 393, height: 852))))
        run.number += 1
        run.events.append(.update(.effect(.act(command: .tap(x: 1, y: 1)), number: run.number)))
        run.events.append(.update(.effectCompleted(.act(command: .tap(x: 1, y: 1)), event: .cancel)))
        run.number += 1
        run.events.append(.update(.effect(.act(command: .tap(x: 2, y: 2)), number: run.number)))
        run.events.append(.update(.finished(RunResult(outcome: Outcome(status: .cancelled, summary: "", steps: 0, durationMs: 0),
                                                      endReason: .cancelled, memory: .saved, effectCount: 3, modelCalls: 0))))
        let t = run.timeline()
        #expect(t.steps.map(\.state) == [.stopped, .stopped])
        #expect(t.steps.allSatisfy { !$0.state.isFailed && $0.state.title == "Stopped" })
        var replay = ReplayState()
        replay.sync(numbers: t.stepNumbers, now: 0)
        replay.stepBack(numbers: t.stepNumbers, now: 0)
        #expect(LiveViewScene.make(timeline: t, replay: replay, now: 0).accessibilityValue.contains("stopped"))
    }

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
