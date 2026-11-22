import Foundation
import Synchronization
import XCTest
import JarvisCore
import JarvisKit

/// A model wrapper that runs `beforeReply` (on the main actor) before answering call
/// number `atCall` (1-based): the lock tests lock the simulator between two steps there.
final class InterposingModel: ModelProvider {
    private let inner: any ModelProvider
    private let atCall: Int
    private let beforeReply: @MainActor @Sendable () async -> Void
    private let calls = Mutex(0)

    init(_ inner: any ModelProvider, atCall: Int, beforeReply: @escaping @MainActor @Sendable () async -> Void) {
        self.inner = inner
        self.atCall = atCall
        self.beforeReply = beforeReply
    }

    var capabilities: ModelCapabilities { inner.capabilities }

    func generate(_ request: ModelRequest) async throws -> String {
        let n = calls.withLock { n -> Int in
            n += 1
            return n
        }
        if n == atCall { await beforeReply() }
        return try await inner.generate(request)
    }
}

/// The phone locks in the middle of a task (simulator, no passcode): the runner refuses the
/// next command as locked, the session waits for an unlock, and after the unlock it looks
/// at the screen again before acting.
final class LockedPhoneAgentTests: QATestCase {
    @MainActor
    func testLockMidTaskWaitsAndResumesAfterUnlock() async throws {
        let app = launchPlayground()
        let run = makeRun("lock-mid-task")
        run.runner.noteActivated(Self.appBundleId)
        let transcript = run.transcript
        let scripted = ScriptedModel(steps: PlaygroundAgentTests.toggleScript)
        // Before the model's second answer (tap the switch) the phone locks; four seconds
        // later the owner unlocks and returns to Jarvis.
        let model = InterposingModel(scripted, atCall: 2) {
            transcript.record("owner", "lock: \(SimulatorLock.lock())")
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(4))
                transcript.record("owner", "unlock: \(SimulatorLock.unlock(returningTo: app))")
            }
        }
        let outcome = try await run.run(task: "Turn on tips in the playground", model: model)
        let entries = transcript.entries

        XCTAssertTrue(entries.contains { $0.kind == "lock" && $0.title.hasPrefix("awaiting unlock") },
                      "the driver never reported waiting for an unlock\n\(outcome.explanation)")
        XCTAssertEqual(outcome.result.outcome.status, .success, outcome.explanation)
        XCTAssertEqual(app.staticTexts["toggles.status"].label, "Tips are on", outcome.explanation)

        // Order: after the wait ended, an observation comes before any command.
        guard let ended = entries.lastIndex(where: { $0.kind == "lock" && $0.title == "unlock wait ended" }) else {
            XCTFail("the unlock wait never ended\n\(outcome.explanation)")
            return
        }
        let after = entries[(ended + 1)...]
        let firstCommand = after.firstIndex { $0.kind == "command" }
        let firstObservation = after.firstIndex { $0.kind == "observation" }
        if let firstCommand {
            XCTAssertNotNil(firstObservation, "a command ran after the unlock with no observation\n\(outcome.explanation)")
            if let firstObservation {
                XCTAssertLessThan(firstObservation, firstCommand, "a command ran after the unlock before the screen was read again\n\(outcome.explanation)")
            }
        }
    }
}
