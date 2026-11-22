import Foundation
import Synchronization
import XCTest
import JarvisCore
import JarvisKit

/// Memory saved by the driver, kept in the test process.
final class CapturedMemory: MemoryPersisting {
    private let saved = Mutex<[String]>([])

    var saves: [String] { saved.withLock { $0 } }

    func save(_ memoryJSON: String) async throws {
        saved.withLock { $0.append(memoryJSON) }
    }
}

enum QAError: Error, CustomStringConvertible {
    case noAnswer(String)

    var description: String {
        switch self {
        case .noAnswer(let q): "the QA owner has no answer for: \(q)"
        }
    }
}

/// The owner in QA runs: decides confirmations by rule, answers questions with an action
/// (e.g. typing a password itself, as a person would), and logs both to the transcript.
final class QAOwner: HumanGate {
    typealias Decide = @Sendable (ConfirmRequest) -> Bool
    typealias Answer = @MainActor @Sendable (String) async -> String?

    private let decide: Decide
    private let onQuestion: Answer?
    private let transcript: Transcript
    private let log = Mutex<(confirms: [ConfirmRequest], questions: [String])>(([], []))

    init(transcript: Transcript, decide: @escaping Decide = { _ in true }, onQuestion: Answer? = nil) {
        self.transcript = transcript
        self.decide = decide
        self.onQuestion = onQuestion
    }

    var confirms: [ConfirmRequest] { log.withLock { $0.confirms } }
    var questions: [String] { log.withLock { $0.questions } }

    func confirm(_ request: ConfirmRequest) async throws -> Bool {
        log.withLock { $0.confirms.append(request) }
        let approved = decide(request)
        transcript.record("owner", "\(approved ? "approved" : "declined") \(request.category) in \(request.app)", request.summary)
        return approved
    }

    func answer(_ question: String) async throws -> String {
        log.withLock { $0.questions.append(question) }
        if let onQuestion, let reply = await onQuestion(question) {
            transcript.record("owner", "answered", reply)
            return reply
        }
        transcript.record("owner", "no answer", question)
        throw QAError.noAnswer(question)
    }
}

/// What one QA run produced.
struct QAOutcome {
    var result: RunResult
    var memoryJSON: String?
    var requests: [ModelRequest]
    var replies: [String?]
    var transcriptJSON: String

    /// Every prompt (system and user parts) and reply, for leak checks.
    var allModelText: String {
        (requests.flatMap { [$0.system, $0.user] } + replies.compactMap { $0 }).joined(separator: "\n")
    }

    /// A failure message that explains the run without opening the artifacts.
    var explanation: String {
        var lines = [
            "status \(result.outcome.status), end \(result.endReason), steps \(result.outcome.steps), model calls \(result.modelCalls)",
            "summary: \(result.outcome.summary)",
        ]
        if let last = replies.last, let reply = last { lines.append("last model reply: \(reply)") }
        if let user = requests.last?.user, let screen = PromptScreen(prompt: user).block {
            lines.append("last screen the model saw:\n\(screen)")
        }
        return lines.joined(separator: "\n")
    }
}

/// One agent run against the simulator: the real core, `SessionDriver`, the in-process
/// XCUITest runner, a model, and complete artifacts.
///
/// Artifacts (all `XCTAttachment`, lifetime `.keepAlways`), per observation `NN`:
/// - `<name>/NN-screen.txt`: the screen block of the prompt built from that observation
///   (exactly what the model read), or why there was none.
/// - `<name>/NN-tree.txt`: the typed tree as the core receives it, then the raw XCUITest
///   snapshot with XCTest type names and raw numbers, plus adapter notes and timing.
/// - `<name>/NN.png`: a screenshot taken with the snapshot.
/// and per run `<name>-transcript.json` (prompts, replies, script reasons, effects,
/// events, owner decisions, commands, timing) and `<name>-result.txt`.
@MainActor
final class QARun {
    let name: String
    let transcript: Transcript
    let runner: XCUITestPhoneRunner
    private weak var testCase: XCTestCase?
    private let prompts = Mutex<[Int: [String]]>([:])
    private var observationNumbers: [Int] = []

    init(name: String, testCase: XCTestCase, runner: XCUITestPhoneRunner = XCUITestPhoneRunner()) {
        self.name = name
        self.testCase = testCase
        self.runner = runner
        self.transcript = Transcript(name: name)
    }

    func run(
        task: String,
        model inner: any ModelProvider,
        owner: QAOwner? = nil,
        memoryJSON: String = "",
        firstUseApproval: Bool = false,
        knownApps: [AppInfo] = KnownApps.common,
        maxSteps: UInt32 = 40,
        maxDurationMs: UInt64 = 240_000
    ) async throws -> QAOutcome {
        let transcript = transcript
        let runner = runner
        runner.onObservation = { [weak self] record in self?.observed(record) }
        runner.onCommand = { command, result in
            transcript.record("command", "\(Transcript.describe(command)) -> \(result.ok ? "ok" : "failed: \(result.error ?? "?")")")
        }
        let prompts = prompts
        let model = RecordingModel(inner, transcript: transcript) { request in
            let n = runner.observationCounter.withLock { $0 }
            prompts.withLock { $0[n, default: []].append(request.user) }
        }
        let owner = owner ?? QAOwner(transcript: transcript)
        let memory = CapturedMemory()
        let config = try SessionConfigBuilder.make(
            capabilities: model.capabilities,
            knownApps: knownApps,
            agentBundleId: "dev.jarvis.app",
            adjustPolicy: { policy in
                policy.firstUseApproval = firstUseApproval
                policy.maxSteps = maxSteps
                policy.maxDurationMs = maxDurationMs
            }
        )
        let deps = DriverDependencies(
            runner: runner, model: model, tools: ToolRegistry(), human: owner, memory: memory,
            screenshots: ScreenshotStore()
        )
        let configuration = DriverConfiguration(
            observeTimeout: .seconds(45), observeAttempts: 2, actTimeout: .seconds(45),
            inferTimeout: .seconds(30), humanTimeout: .seconds(60)
        )
        transcript.record("task", "run \(name)", task)
        let driver = try SessionDriver(
            config: config, task: TaskSpec(id: name, text: task), memoryJSON: memoryJSON,
            dependencies: deps, configuration: configuration
        )
        let collector = Task {
            for await update in driver.updates { transcript.record(update) }
        }
        let result = await driver.run()
        await collector.value
        runner.onObservation = nil
        runner.onCommand = nil

        attachScreens()
        let json = String(decoding: transcript.jsonData(), as: UTF8.self)
        attach(XCTAttachment(data: transcript.jsonData(), uniformTypeIdentifier: "public.json"), "\(name)-transcript.json")
        let outcome = QAOutcome(
            result: result, memoryJSON: memory.saves.last, requests: model.requests, replies: model.replies,
            transcriptJSON: json
        )
        attach(XCTAttachment(string: Transcript.describe(result) + "\n\n" + outcome.explanation), "\(name)-result.txt")
        return outcome
    }

    /// Notes an interruption (system alert) seen by an XCTest interruption monitor.
    func recordInterruption(_ text: String) {
        transcript.record("interruption", "system alert", text)
    }

    // MARK: - Artifacts

    private func observed(_ record: RunnerObservationRecord) {
        observationNumbers.append(record.number)
        let n = String(format: "%02d", record.number)
        var header = [
            "observation \(record.number): \(record.bundleId)",
            "screen \(Int(record.raw.frame.width))x\(Int(record.raw.frame.height)) pt, keyboard \(record.built.keyboardVisible ? "visible" : "hidden")",
            "nodes \(record.built.nodeCount), XCUITest snapshot \(record.snapshotMs) ms",
        ]
        if !record.built.unmappedTypes.isEmpty {
            header.append("types without a core role: " + record.built.unmappedTypes.sorted { $0.key < $1.key }.map { "\($0.key)×\($0.value)" }.joined(separator: ", "))
        }
        header += record.notes.map { "note: \($0)" }
        let text = header.joined(separator: "\n")
            + "\n\n== typed tree (what the core receives) ==\n" + UiTreeBuilder.dump(record.built.root)
            + "\n\n== raw XCUITest snapshot (type name and raw number) ==\n" + UiTreeBuilder.dump(record.raw) + "\n"
        attach(XCTAttachment(string: text), "\(name)/\(n)-tree.txt")
        if let shot = record.screenshot {
            attach(XCTAttachment(screenshot: shot), "\(name)/\(n).png")
        }
        transcript.record("observation", "#\(record.number) \(record.bundleId), \(record.built.nodeCount) nodes, \(record.snapshotMs) ms", record.notes.isEmpty ? nil : record.notes.joined(separator: "\n"))
    }

    private func attachScreens() {
        let byObservation = prompts.withLock { $0 }
        for number in observationNumbers {
            let n = String(format: "%02d", number)
            let text: String
            if let users = byObservation[number], !users.isEmpty {
                text = users.enumerated().map { i, user in
                    let block = PromptScreen(prompt: user).block ?? "(the prompt had no screen block)"
                    return "prompt \(i + 1) built from observation \(number) — the screen as the model read it:\n\(block)"
                }.joined(separator: "\n\n") + "\n"
            } else {
                text = "observation \(number): no model call was built from it (a verification after an action, a replayed flow step, or the final state). The typed tree is in \(n)-tree.txt.\n"
            }
            attach(XCTAttachment(string: text), "\(name)/\(n)-screen.txt")
        }
    }

    private func attach(_ attachment: XCTAttachment, _ name: String) {
        attachment.name = name
        attachment.lifetime = .keepAlways
        testCase?.add(attachment)
    }
}

/// Shared set-up for the agent tests.
class QATestCase: XCTestCase {
    static let appBundleId = "dev.jarvis.app"
    /// The run whose transcript receives interruption notes.
    var currentRun: QARun?

    override func setUpWithError() throws {
        try super.setUpWithError()
        continueAfterFailure = false
        addUIInterruptionMonitor(withDescription: "System alert (logged; the agent handles it)") { [weak self] element in
            let text = element.label
            MainActor.assumeIsolated { self?.currentRun?.recordInterruption(text) }
            return false
        }
    }

    /// Launches the Jarvis app on the Playground fixtures with fresh state.
    @MainActor
    func launchPlayground() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-jarvis-playground", "-jarvis-reset"]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 20), "the Jarvis app did not reach the foreground (state \(app.state.rawValue))")
        return app
    }

    @MainActor
    func makeRun(_ name: String, runner: XCUITestPhoneRunner = XCUITestPhoneRunner()) -> QARun {
        let run = QARun(name: name, testCase: self, runner: runner)
        currentRun = run
        return run
    }

    /// Waits for `element` and fails with what is on screen when it does not appear.
    @MainActor
    func require(_ element: XCUIElement, _ what: String, timeout: TimeInterval = 10, in app: XCUIApplication,
                 file: StaticString = #filePath, line: UInt = #line) {
        if element.waitForExistence(timeout: timeout) { return }
        let tree = (try? app.snapshot()).map { UiTreeBuilder.dump(UiTreeBuilder.build(SnapshotCopy.copy($0), screenSize: nil).root) } ?? "(no snapshot)"
        XCTFail("\(what) did not appear within \(timeout) s. Screen:\n\(tree)", file: file, line: line)
    }
}
