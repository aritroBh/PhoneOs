import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// Cancellation reaches the core promptly, also while IO that ignores cancellation is in
/// flight, and memory is saved exactly once.
@Suite("SessionDriver cancellation", .timeLimit(.minutes(1)))
struct DriverCancellationTests {
    private func expectCancelled(_ result: RunResult, _ h: Harness) {
        #expect(result.outcome.status == .cancelled)
        #expect(result.endReason == .cancelled)
        #expect(result.memory == .saved)
        #expect(h.memory.saves.count == 1)
    }

    @Test("cancel() mid-inference")
    func cancelMidInference() async throws {
        var h = Harness()
        let latch = Latch()
        h.model = FakeModel { _, _ in
            await latch.hang()
            return doneJSON
        }
        let driver = try await h.makeDriver()
        let run = Task { await driver.run() }
        try await waitUntil { latch.entered == 1 }
        let started = ContinuousClock.now
        await driver.cancel()
        let result = try await value(of: run, "the cancelled run to end")
        #expect(ContinuousClock.now - started < .seconds(2))
        latch.release()
        expectCancelled(result, h)
    }

    @Test("task cancellation mid-inference")
    func taskCancelMidInference() async throws {
        var h = Harness()
        let latch = Latch()
        h.model = FakeModel { _, _ in
            await latch.hang()
            return doneJSON
        }
        let driver = try await h.makeDriver()
        let run = Task { await driver.run() }
        try await waitUntil { latch.entered == 1 }
        let started = ContinuousClock.now
        run.cancel()
        let result = try await value(of: run, "the cancelled run to end")
        #expect(ContinuousClock.now - started < .seconds(2))
        latch.release()
        expectCancelled(result, h)
    }

    @Test("cancel() mid-action")
    func cancelMidAction() async throws {
        var h = Harness()
        let latch = Latch()
        h.phone = FakePhone(commandHang: latch)
        let driver = try await h.makeDriver()
        let run = Task { await driver.run() }
        try await waitUntil { latch.entered == 1 }
        await driver.cancel()
        let result = try await value(of: run, "the cancelled run to end")
        latch.release()
        expectCancelled(result, h)
        #expect(h.model.callCount == 1)
    }

    @Test("task cancellation mid-observation")
    func taskCancelMidObservation() async throws {
        var h = Harness()
        let latch = Latch()
        h.phone = FakePhone(snapshotHang: latch)
        let driver = try await h.makeDriver()
        let run = Task { await driver.run() }
        try await waitUntil { latch.entered == 1 }
        run.cancel()
        let result = try await value(of: run, "the cancelled run to end")
        latch.release()
        expectCancelled(result, h)
        #expect(h.model.callCount == 0)
    }

    @Test("cancel() while waiting for a confirmation")
    func cancelMidConfirmation() async throws {
        var h = Harness()
        let latch = Latch()
        h.gate = FakeGate(confirmReply: .hang(latch))
        let driver = try await h.makeDriver()
        let run = Task { await driver.run() }
        try await waitUntil { latch.entered == 1 }
        await driver.cancel()
        let result = try await value(of: run, "the cancelled run to end")
        latch.release()
        expectCancelled(result, h)
        // On runner text the first question is already the row tap: nothing ran.
        #expect(h.gate.confirms.first?.category == .unverified)
        #expect(h.phone.page == .settings, "the asked tap never ran")
    }

    @Test("cancel() before run() ends the run at once")
    func cancelBeforeRun() async throws {
        let h = Harness()
        let driver = try await h.makeDriver()
        await driver.cancel()
        let result = await driver.run()
        expectCancelled(result, h)
        #expect(h.phone.snapshots == 0)
        #expect(h.model.callCount == 0)
    }
}
