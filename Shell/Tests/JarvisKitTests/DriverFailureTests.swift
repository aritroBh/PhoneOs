import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// `SessionDriver` against the real core: each IO failure maps to the right event.
@Suite("SessionDriver failure mapping")
struct DriverFailureTests {
    @Test("a runner command error becomes a failed ActResult and the run recovers")
    func runnerCommandError() async throws {
        let h = Harness()
        h.phone.state.withLock { $0.failingCommands = 1 }
        let (result, updates) = try await h.run()

        #expect(result.outcome.status == .success)
        #expect(updates.contains(.ioFailed(.act, message: "runner connection reset", willRetry: false)))
        let failedAct = updates.contains { update in
            if case .effectCompleted(.act, .actResult(let r)) = update {
                return !r.ok && r.error == "runner connection reset"
            }
            return false
        }
        #expect(failedAct)
        #expect(h.model.requests.contains { $0.user.contains("runner connection reset") }, "the model is told")
        #expect(h.memory.saves.count == 1)
    }

    @Test("a transient snapshot error is retried")
    func transientSnapshotError() async throws {
        let h = Harness()
        h.phone.state.withLock { $0.failingSnapshots = 1 }
        let (result, updates) = try await h.run()
        #expect(result.outcome.status == .success)
        #expect(updates.contains(.ioFailed(.observe, message: "runner snapshot failed", willRetry: true)))
        #expect(h.memory.saves.count == 1)
    }

    @Test("a runner that cannot be read ends the run as runnerUnavailable")
    func runnerUnavailable() async throws {
        let h = Harness()
        h.phone.state.withLock { $0.failingSnapshots = 100 }
        let (result, updates) = try await h.run()
        #expect(result.endReason == .runnerUnavailable("runner snapshot failed"))
        #expect(result.outcome.status == .cancelled)
        #expect(h.phone.snapshots == 3)
        #expect(updates.filter(\.isIOFailure).count == 3)
        #expect(h.model.callCount == 0)
        #expect(h.memory.saves.count == 1)
    }

    @Test("a model error becomes ModelFailed and the core finishes failed")
    func modelError() async throws {
        var h = Harness()
        h.model = FakeModel { _, _ in throw InjectedError(message: "out of memory") }
        let (result, _) = try await h.run()
        #expect(result.outcome.status == .failed)
        #expect(result.outcome.summary.contains("out of memory"))
        #expect(result.endReason == .sessionFinished)
        #expect(h.memory.saves.count == 1)
    }

    @Test("a model that does not answer in time times out")
    func modelTimeout() async throws {
        var h = Harness()
        let latch = Latch()
        h.model = FakeModel { _, _ in
            await latch.hang()
            return doneJSON
        }
        h.configuration.inferTimeout = .milliseconds(150)
        let started = ContinuousClock.now
        let (result, _) = try await h.run()
        latch.release()
        #expect(ContinuousClock.now - started < .seconds(3))
        #expect(result.outcome.status == .failed)
        #expect(result.outcome.summary.contains("timed out"))
        #expect(h.memory.saves.count == 1)
    }

    @Test("a hung runner command times out as a failed ActResult")
    func actTimeout() async throws {
        var h = Harness()
        let latch = Latch()
        h.phone = FakePhone(commandHang: latch)
        h.configuration.actTimeout = .milliseconds(100)
        h.model = FakeModel { request, call in
            if call == 1 { return try await wifiOffScript(request, call) }
            #expect(request.user.contains("timed out"))
            return #"{"thought":"stop","action":"fail","reason":"runner stuck"}"#
        }
        let (result, _) = try await h.run()
        latch.release()
        #expect(result.outcome.status == .failed)
        #expect(result.outcome.summary == "runner stuck")
        #expect(h.memory.saves.count == 1)
    }

    @Test("a failing confirmation UI counts as a decline")
    func confirmationFailure() async throws {
        var h = Harness()
        h.gate = FakeGate(confirmReply: .fail)
        h.model = FakeModel { request, call in
            if request.user.contains("the owner declined") {
                return #"{"thought":"stop","action":"fail","reason":"not confirmed"}"#
            }
            return try await wifiOffScript(request, call)
        }
        let (result, updates) = try await h.run()
        #expect(result.outcome.status == .failed)
        #expect(h.phone.page == .wifiOn)
        #expect(updates.contains(.ioFailed(.confirm, message: "no confirmation UI", willRetry: false)))
        #expect(h.memory.saves.count == 1)
    }

    @Test("an unanswered question stops the run as noAnswer")
    func questionFailure() async throws {
        var h = Harness()
        h.gate = FakeGate(answerReply: .fail)
        h.model = FakeModel { _, _ in #"{"thought":"?","action":"ask_user","question":"Which one?"}"# }
        let (result, _) = try await h.run()
        #expect(result.endReason == .noAnswer("notification dismissed"))
        #expect(result.outcome.status == .cancelled)
        #expect(h.model.callCount == 1)
        #expect(h.memory.saves.count == 1)
    }

    @Test("an unknown tool is a failed ToolResult, never a throw")
    func unknownTool() async throws {
        var h = Harness()
        // Registered for the config, then removed, so the core accepts the call but the
        // host no longer knows it.
        SampleTools.registerEcho(in: h.tools)
        let tools = h.tools
        h.model = FakeModel { request, call in
            if call == 1 {
                tools.unregister("echo")
                return #"{"thought":"tool","action":"call_tool","name":"echo","args":{"text":"x"}}"#
            }
            #expect(request.user.contains("unknown tool"))
            return doneJSON
        }
        let (result, updates) = try await h.run()
        #expect(result.outcome.status == .success)
        #expect(updates.contains { update in
            if case .effectCompleted(.callTool, .toolResult(let r)) = update { return !r.ok }
            return false
        })
    }
}
