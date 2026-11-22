import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// The order of `SessionDriver.updates` documented on `DriverUpdate`.
@Suite("SessionDriver update stream")
struct DriverUpdateStreamTests {
    @Test("updates come in the documented order")
    func ordering() async throws {
        let h = Harness()
        let (result, updates) = try await h.run()
        #expect(result.outcome.status == .success)

        #expect(updates.first == .started(h.task))
        #expect(updates.last == .finished(result))
        #expect(updates.filter { if case .finished = $0 { return true } else { return false } }.count == 1)

        // Walk the stream: each .effect(n) is followed by .progress, then (unless it is the
        // final .finished effect) its own updates and exactly one .effectCompleted for it.
        var expectedNumber = 1
        var open: Effect?
        var sawProgressForOpen = false
        var pendingConfirm = false
        for update in updates.dropFirst().dropLast() {
            switch update {
            case .effect(let effect, let number):
                #expect(open == nil, "previous effect completed before the next")
                #expect(number == expectedNumber)
                expectedNumber += 1
                if case .finished = effect {
                    open = nil
                } else {
                    open = effect
                }
                sawProgressForOpen = false
            case .progress:
                sawProgressForOpen = true
            case .confirmationPending(let request):
                #expect(open == .confirm(request: request))
                #expect(sawProgressForOpen)
                pendingConfirm = true
            case .effectCompleted(let effect, _):
                #expect(effect == open)
                #expect(sawProgressForOpen)
                if case .confirm = effect {
                    #expect(pendingConfirm, "confirmationPending precedes the decision")
                    pendingConfirm = false
                }
                open = nil
            case .questionPending, .ioFailed, .note:
                #expect(open != nil)
            case .started, .finished:
                Issue.record("\(update) in the middle of the stream")
            }
        }
        #expect(expectedNumber - 1 == result.effectCount)
        #expect(open == nil)
    }

    @Test("the stream ends after .finished, also on cancellation")
    func streamEnds() async throws {
        let h = Harness()
        let driver = try await h.makeDriver()
        await driver.cancel()
        let collector = Task { () -> [DriverUpdate] in
            var all: [DriverUpdate] = []
            for await update in driver.updates { all.append(update) }
            return all
        }
        let result = await driver.run()
        let updates = await collector.value
        #expect(updates.first == .started(h.task))
        #expect(updates.last == .finished(result))
        #expect(updates.count == 4) // started, effect(finished), progress, finished
    }

    @Test("the core's runner-text parse issue is reported as a note")
    func observationIssueNote() async throws {
        var h = Harness()
        h.phone = FakePhone(sourceOverride: "<<garbage>>")
        h.model = FakeModel { _, _ in #"{"thought":"no","action":"fail","reason":"nothing"}"# }
        let (_, updates) = try await h.run()
        #expect(updates.contains { if case .note(let n) = $0 { return n.contains("empty screen") } else { return false } })
    }
}
