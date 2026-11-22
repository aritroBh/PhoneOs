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
        // Both raw lock readings go into every observation's notes for this test.
        let run = makeRun("lock-mid-task", runner: XCUITestPhoneRunner(
            configuration: XCUITestPhoneRunner.Configuration(noteLockSignalsEveryObservation: true)
        ))
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
        // A lock reading that never clears ends the run as Interrupted after 20 s (the
        // harness default), and the whole run is capped at 60 s.
        let outcome = try await run.run(task: "Turn on tips in the playground", model: model, maxDurationMs: 60_000)
        let entries = transcript.entries
        let history = "lock signal history:\n" + run.runner.lockSignalHistory.joined(separator: "\n")

        // After the unlock the app must be back in front and show the result within 30 s.
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30),
                      "the Jarvis app is not in front after the unlock (state \(app.state.rawValue))\n\(history)")
        XCTAssertTrue(app.staticTexts["toggles.status"].waitForExistence(timeout: 30),
                      "the toggles screen is not showing after the unlock\n\(history)\n\(outcome.explanation)")
        XCTAssertNotEqual(outcome.result.outcome.status, .interrupted,
                          "the session stayed in its unlock wait until the limit: a lock reading never cleared\n\(history)\n\(outcome.explanation)")

        XCTAssertTrue(entries.contains { $0.kind == "lock" && $0.title.hasPrefix("awaiting unlock") },
                      "the driver never reported waiting for an unlock\n\(history)\n\(outcome.explanation)")
        XCTAssertEqual(outcome.result.outcome.status, .success, "\(history)\n\(outcome.explanation)")
        XCTAssertEqual(app.staticTexts["toggles.status"].label, "Tips are on", "\(history)\n\(outcome.explanation)")

        // Order: after the wait ended, an observation comes before any command.
        guard let ended = entries.lastIndex(where: { $0.kind == "lock" && $0.title == "unlock wait ended" }) else {
            XCTFail("the unlock wait never ended\n\(history)\n\(outcome.explanation)")
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
