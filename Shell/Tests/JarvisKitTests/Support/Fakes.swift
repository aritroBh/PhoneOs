import Foundation
import JarvisCore
import Synchronization
@testable import JarvisKit

// MARK: - Waiting helpers

/// A switch that ends a deliberate hang. `hang()` ignores task cancellation on purpose (it
/// suspends on a continuation that only `release()` resumes), to prove the driver does not
/// depend on IO being cooperative.
final class Latch: Sendable {
    private struct State {
        var entered = 0
        var released = false
        var waiters: [CheckedContinuation<Void, Never>] = []
    }

    private let state = Mutex(State())

    var entered: Int { state.withLock { $0.entered } }

    func release() {
        let waiters = state.withLock { s -> [CheckedContinuation<Void, Never>] in
            s.released = true
            defer { s.waiters = [] }
            return s.waiters
        }
        waiters.forEach { $0.resume() }
    }

    func hang() async {
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            let released = state.withLock { s -> Bool in
                s.entered += 1
                if !s.released { s.waiters.append(continuation) }
                return s.released
            }
            if released { continuation.resume() }
        }
    }
}

struct WaitTimeout: Error {}

/// Polls `condition` every 10 ms until it holds or `seconds` pass.
func waitUntil(seconds: Double = 5, _ condition: @Sendable () async -> Bool) async throws {
    let deadline = ContinuousClock.now + .milliseconds(Int(seconds * 1_000))
    while ContinuousClock.now < deadline {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    throw WaitTimeout()
}

// MARK: - A fake Settings app driven through runner text

struct InjectedError: Error, CustomStringConvertible {
    var message: String
    var description: String { message }
}

enum Page: Sendable, Equatable { case settings, wifiOn, wifiOff }

let settingsBundle = "com.apple.Preferences"
let screenSize = Size(width: 393, height: 852)

/// The Settings app as LoopCtrl-style runner text: a Wi-Fi cell, then a Wi-Fi switch.
final class FakePhone: PhoneRunner {
    struct State {
        var page = Page.settings
        var commands: [RunnerCommand] = []
        var snapshots = 0
        var screenshotRequests = 0
        /// The next N snapshots throw.
        var failingSnapshots = 0
        /// The next N commands throw.
        var failingCommands = 0
    }

    let state = Mutex(State())
    /// When set, `perform` hangs on it (ignoring cancellation).
    let commandHang: Latch?
    /// When set, `snapshot` hangs on it.
    let snapshotHang: Latch?
    /// When set, every snapshot returns this text instead of the page's tree.
    let sourceOverride: String?

    init(commandHang: Latch? = nil, snapshotHang: Latch? = nil, sourceOverride: String? = nil) {
        self.commandHang = commandHang
        self.snapshotHang = snapshotHang
        self.sourceOverride = sourceOverride
    }

    var page: Page { state.withLock { $0.page } }
    var commands: [RunnerCommand] { state.withLock { $0.commands } }
    var snapshots: Int { state.withLock { $0.snapshots } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        if let snapshotHang { await snapshotHang.hang() }
        let (page, fail) = state.withLock { s -> (Page, Bool) in
            s.snapshots += 1
            if includeScreenshot { s.screenshotRequests += 1 }
            if s.failingSnapshots > 0 {
                s.failingSnapshots -= 1
                return (s.page, true)
            }
            return (s.page, false)
        }
        if fail { throw InjectedError(message: "runner snapshot failed") }
        return RunnerObservation(
            source: sourceOverride ?? Self.text(for: page),
            appBundleId: settingsBundle,
            appName: "Settings",
            screenSize: screenSize,
            screenshot: includeScreenshot
                ? CapturedScreenshot(data: Data([0xFF, 0xD8, 0xFF]), mimeType: "image/jpeg", pixelWidth: 786, pixelHeight: 1704, scale: 2)
                : nil
        )
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        if let commandHang { await commandHang.hang() }
        if apply(command) { throw InjectedError(message: "runner connection reset") }
        return RunnerResult(ok: true, error: nil)
    }

    /// Records `command` and moves between pages; returns true when it must fail.
    @discardableResult
    func apply(_ command: RunnerCommand) -> Bool {
        state.withLock { s -> Bool in
            s.commands.append(command)
            if s.failingCommands > 0 {
                s.failingCommands -= 1
                return true
            }
            if case .tap(let x, let y) = command {
                switch s.page {
                case .settings where (16..<377).contains(x) && (200..<244).contains(y):
                    s.page = .wifiOn
                case .wifiOn where (312..<363).contains(x) && (166..<197).contains(y):
                    s.page = .wifiOff
                default:
                    break
                }
            }
            return false
        }
    }

    static func text(for page: Page) -> String {
        switch page {
        case .settings:
            return """
            Application, label: 'Settings', {{0, 0}, {393, 852}}
              Window, {{0, 0}, {393, 852}}
                NavigationBar, id: 'Settings', {{0, 59}, {393, 81}}
                  StaticText, label: 'Settings', {{16, 96}, {140, 41}}
                Table, {{0, 140}, {393, 712}}
                  Cell, {{16, 200}, {361, 44}}
                    StaticText, label: 'Wi-Fi', {{60, 212}, {200, 20}}
                    StaticText, label: 'HomeNet', {{260, 212}, {100, 20}}
                  Cell, {{16, 244}, {361, 44}}
                    StaticText, label: 'Bluetooth', {{60, 256}, {200, 20}}
                    StaticText, label: 'On', {{260, 256}, {100, 20}}
            """
        case .wifiOn, .wifiOff:
            return """
            Application, label: 'Settings', {{0, 0}, {393, 852}}
              Window, {{0, 0}, {393, 852}}
                NavigationBar, id: 'Wi-Fi', {{0, 59}, {393, 81}}
                  Button, label: 'Settings', id: 'BackButton', {{8, 70}, {90, 44}}
                Table, {{0, 140}, {393, 712}}
                  Cell, {{16, 160}, {361, 44}}
                    Switch, label: 'Wi-Fi', value: '\(page == .wifiOn ? 1 : 0)', {{312, 166}, {51, 31}}
            """
        }
    }
}

// MARK: - Model

/// The element number of the prompt line `[N] <role> "<label…`.
func elementIndex(_ prompt: String, role: String, label: String) -> UInt32? {
    let needle = "] \(role) \"\(label)"
    for line in prompt.split(separator: "\n") where line.contains(needle) {
        let trimmed = line.drop(while: { $0 == " " })
        guard trimmed.first == "[", let close = trimmed.firstIndex(of: "]") else { continue }
        return UInt32(trimmed[trimmed.index(after: trimmed.startIndex)..<close])
    }
    return nil
}

func tapJSON(_ element: UInt32) -> String {
    #"{"thought":"tap","action":"tap","element":\#(element)}"#
}

let doneJSON = #"{"thought":"done","action":"done","summary":"Wi-Fi is off"}"#

/// Turns Wi-Fi off by reading the prompt: tap the Wi-Fi cell, then the switch, then done.
@Sendable func wifiOffScript(_ request: ModelRequest, _ call: Int) async throws -> String {
    if request.user.contains("Switch \"Wi-Fi\"") && request.user.contains("value=\"1\""),
       let i = elementIndex(request.user, role: "Switch", label: "Wi-Fi") {
        return tapJSON(i)
    }
    if let i = elementIndex(request.user, role: "Cell", label: "Wi-Fi") {
        return tapJSON(i)
    }
    return doneJSON
}

final class FakeModel: ModelProvider {
    typealias Script = @Sendable (ModelRequest, Int) async throws -> String

    let capabilities: ModelCapabilities
    private let script: Script
    private let recorded = Mutex<[ModelRequest]>([])

    init(capabilities: ModelCapabilities = ModelCapabilities(contextTokens: 4_096, supportsVision: false), script: @escaping Script = wifiOffScript) {
        self.capabilities = capabilities
        self.script = script
    }

    var requests: [ModelRequest] { recorded.withLock { $0 } }
    var callCount: Int { recorded.withLock { $0.count } }

    func generate(_ request: ModelRequest) async throws -> String {
        let n = recorded.withLock { r -> Int in
            r.append(request)
            return r.count
        }
        return try await script(request, n)
    }
}

// MARK: - Owner

final class FakeGate: HumanGate {
    enum Reply: Sendable {
        case value
        case fail
        case hang(Latch)
    }

    let approve: Bool
    let answerText: String
    let confirmReply: Reply
    let answerReply: Reply
    private let log = Mutex<(confirms: [ConfirmRequest], questions: [String])>(([], []))
    private let hook = Mutex<(@Sendable () async -> Void)?>(nil)

    /// Runs at the start of every `confirm` (after it is recorded).
    var beforeConfirm: (@Sendable () async -> Void)? {
        get { hook.withLock { $0 } }
        set { hook.withLock { $0 = newValue } }
    }

    init(approve: Bool = true, answer: String = "HomeNet", confirmReply: Reply = .value, answerReply: Reply = .value) {
        self.approve = approve
        self.answerText = answer
        self.confirmReply = confirmReply
        self.answerReply = answerReply
    }

    var confirms: [ConfirmRequest] { log.withLock { $0.confirms } }
    var questions: [String] { log.withLock { $0.questions } }

    func confirm(_ request: ConfirmRequest) async throws -> Bool {
        log.withLock { $0.confirms.append(request) }
        if let beforeConfirm { await beforeConfirm() }
        switch confirmReply {
        case .value: return approve
        case .fail: throw InjectedError(message: "no confirmation UI")
        case .hang(let latch):
            await latch.hang()
            return approve
        }
    }

    func answer(_ question: String) async throws -> String {
        log.withLock { $0.questions.append(question) }
        switch answerReply {
        case .value: return answerText
        case .fail: throw InjectedError(message: "notification dismissed")
        case .hang(let latch):
            await latch.hang()
            return answerText
        }
    }
}

// MARK: - Memory

final class RecordingMemory: MemoryPersisting {
    private let saved = Mutex<[String]>([])
    let failing: Bool

    init(failing: Bool = false) { self.failing = failing }

    var saves: [String] { saved.withLock { $0 } }

    func save(_ memoryJSON: String) async throws {
        saved.withLock { $0.append(memoryJSON) }
        if failing { throw InjectedError(message: "disk full") }
    }
}

// MARK: - Clock

/// A clock that advances 50 ms on every read (deterministic event timestamps).
final class SteppingClock: MonotonicClock {
    private let now = Mutex<UInt64>(1_000)

    func nowMs() -> UInt64 {
        now.withLock { value in
            defer { value += 50 }
            return value
        }
    }
}

// MARK: - Assembly

struct Harness {
    var phone = FakePhone()
    var model = FakeModel()
    var gate = FakeGate()
    var memory = RecordingMemory()
    var tools = ToolRegistry()
    var screenshots = ScreenshotStore()
    var configuration = DriverConfiguration(observeRetryDelay: .milliseconds(10))
    var task = TaskSpec(id: "t1", text: "Turn off Wi-Fi")
    /// The core asks once per app before first use. Off here so each test sees only the
    /// confirmation it is about; `firstUseApprovalIsAsked` turns it on.
    var firstUseApproval = false

    func makeDriver(memoryJSON: String = "") async throws -> SessionDriver {
        let config = try SessionConfigBuilder.make(
            capabilities: model.capabilities,
            tools: await tools.toolSpecs(),
            knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")],
            adjustPolicy: { [firstUseApproval] in $0.firstUseApproval = firstUseApproval }
        )
        let deps = DriverDependencies(
            runner: phone, model: model, tools: tools, human: gate, memory: memory,
            screenshots: screenshots, clock: SteppingClock()
        )
        return try SessionDriver(config: config, task: task, memoryJSON: memoryJSON, dependencies: deps, configuration: configuration)
    }

    /// Runs one session and collects every update.
    func run(memoryJSON: String = "") async throws -> (RunResult, [DriverUpdate]) {
        let driver = try await makeDriver(memoryJSON: memoryJSON)
        let collector = Task { () -> [DriverUpdate] in
            var all: [DriverUpdate] = []
            for await update in driver.updates { all.append(update) }
            return all
        }
        let result = await driver.run()
        return (result, await collector.value)
    }
}

extension DriverUpdate {
    var isIOFailure: Bool {
        if case .ioFailed = self { return true }
        return false
    }
}
