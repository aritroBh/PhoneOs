import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// `SessionDriver` against the real Rust core: the paths where IO works.
@Suite("SessionDriver with the real core")
struct DriverTests {
    @Test("full success path with runner-text observations")
    func successPath() async throws {
        let h = Harness()
        let (result, _) = try await h.run()

        #expect(result.outcome.status == .success)
        #expect(result.outcome.summary == "Wi-Fi is off")
        #expect(result.endReason == .sessionFinished)
        #expect(result.modelCalls == 3)
        #expect(h.model.callCount == 3)
        #expect(h.phone.page == .wifiOff)
        #expect(h.phone.commands.count == 2)
        #expect(h.phone.snapshots >= 3)
        #expect(h.gate.confirms.count == 1)
        #expect(h.gate.confirms.first?.category == .systemSetting)
        #expect(h.gate.confirms.first?.app == settingsBundle)
        #expect(result.memory == .saved)
        #expect(h.memory.saves.count == 1)
        let memory = try JSONValue(parsing: try #require(h.memory.saves.first))
        #expect(memory["flows"]?["flows"]?.arrayValue?.count == 1)
    }

    @Test("first use of an app asks once, and the approval is remembered in memory")
    func firstUseApprovalIsAsked() async throws {
        var first = Harness()
        first.firstUseApproval = true
        let (result, _) = try await first.run()
        #expect(result.outcome.status == .success)
        #expect(first.gate.confirms.map(\.category) == [.firstUse, .systemSetting])
        #expect(first.gate.confirms.first?.app == settingsBundle)

        var second = Harness()
        second.firstUseApproval = true
        let (again, _) = try await second.run(memoryJSON: try #require(first.memory.saves.first))
        #expect(again.outcome.status == .success)
        #expect(second.gate.confirms.map(\.category) == [.systemSetting])
    }

    @Test("a second run from the saved memory replays with zero model calls")
    func replayFromMemory() async throws {
        let first = Harness()
        let (firstResult, _) = try await first.run()
        #expect(firstResult.outcome.status == .success)
        let saved = try #require(first.memory.saves.first)

        let second = Harness()
        let (result, _) = try await second.run(memoryJSON: saved)
        #expect(result.outcome.status == .success)
        #expect(result.modelCalls == 0)
        #expect(second.model.callCount == 0)
        #expect(second.phone.page == .wifiOff)
        #expect(second.gate.confirms.count == 1, "the replay passes the same confirmation gate")
        #expect(second.memory.saves.count == 1)
    }

    @Test("a declined confirmation is never executed and the model is told")
    func confirmationDeclined() async throws {
        var h = Harness()
        h.gate = FakeGate(approve: false)
        h.model = FakeModel { request, call in
            if request.user.contains("the owner declined") {
                return #"{"thought":"stop","action":"fail","reason":"owner declined"}"#
            }
            return try await wifiOffScript(request, call)
        }
        let (result, updates) = try await h.run()

        #expect(result.outcome.status == .failed)
        #expect(result.outcome.summary == "owner declined")
        #expect(h.gate.confirms.count == 1)
        #expect(h.phone.page == .wifiOn, "the switch was not tapped")
        #expect(h.phone.commands.count == 1)
        #expect(updates.contains(.effectCompleted(.confirm(request: try #require(h.gate.confirms.first)), event: .userDecision(approved: false))))
        #expect(h.memory.saves.count == 1)
    }

    @Test("ask_user: the question reaches the owner and the answer reaches the model")
    func askUser() async throws {
        var h = Harness()
        h.gate = FakeGate(answer: "the home network")
        h.model = FakeModel { request, call in
            if call == 1 {
                return #"{"thought":"unclear","action":"ask_user","question":"Which network?"}"#
            }
            #expect(request.user.contains("the home network"))
            return #"{"thought":"ok","action":"done","summary":"answered"}"#
        }
        let (result, updates) = try await h.run()

        #expect(result.outcome.status == .success)
        #expect(h.gate.questions == ["Which network?"])
        #expect(updates.contains(.questionPending("Which network?")))
        #expect(h.model.callCount == 2)
        #expect(h.memory.saves.count == 1)
    }

    @Test("tool call round trip through ToolRegistry")
    func toolCall() async throws {
        var h = Harness()
        SampleTools.registerEcho(in: h.tools)
        let utc = TimeZone(secondsFromGMT: 0) ?? .current
        SampleTools.registerClockNow(in: h.tools, now: { Date(timeIntervalSince1970: 0) }, timeZone: utc)
        h.model = FakeModel { request, call in
            switch call {
            case 1:
                #expect(request.user.contains("echo"))
                return #"{"thought":"tool","action":"call_tool","name":"echo","args":{"text":"hello tools"}}"#
            case 2:
                #expect(request.user.contains("hello tools"))
                return #"{"thought":"tool","action":"call_tool","name":"clock_now","args":{}}"#
            default:
                #expect(request.user.contains("1970-01-01T00:00:00Z"))
                return #"{"thought":"done","action":"done","summary":"tools ok"}"#
            }
        }
        let (result, updates) = try await h.run()

        #expect(result.outcome.status == .success)
        #expect(result.outcome.summary == "tools ok")
        let toolResults = updates.compactMap { update -> ToolResult? in
            if case .effectCompleted(.callTool, .toolResult(let r)) = update { return r }
            return nil
        }
        #expect(toolResults == [
            ToolResult(ok: true, output: "hello tools", error: nil),
            ToolResult(ok: true, output: "1970-01-01T00:00:00Z (\(utc.identifier))", error: nil),
        ])
        #expect(h.memory.saves.count == 1)
    }

    @Test("run() called twice concurrently runs the session once")
    func reentrantRun() async throws {
        let h = Harness()
        let driver = try await h.makeDriver()
        async let a = driver.run()
        async let b = driver.run()
        let (ra, rb) = await (a, b)
        let rc = await driver.run()
        #expect(ra == rb)
        #expect(rb == rc)
        #expect(ra.outcome.status == .success)
        #expect(h.model.callCount == 3)
        #expect(h.memory.saves.count == 1)
    }

    @Test("invalid memory throws at init; quarantine and start fresh")
    func invalidMemory() async throws {
        let h = Harness()
        await #expect(throws: (any Error).self) {
            _ = try await h.makeDriver(memoryJSON: #"{"schema_version": 999}"#)
        }
        let driver = try await h.makeDriver(memoryJSON: "")
        let result = await driver.run()
        #expect(result.outcome.status == .success)
    }

    @Test("vision request: the screenshot lands in the store and its handle reaches the model")
    func visionScreenshot() async throws {
        var h = Harness()
        h.model = FakeModel(capabilities: ModelCapabilities(contextTokens: 4_096, supportsVision: true)) { request, call in
            if call == 1 { return #"{"thought":"look","action":"look"}"# }
            let image = try #require(request.image)
            #expect(request.kind == .vision)
            #expect(image.width == 786 && image.height == 1704)
            return doneJSON
        }
        let screenshots = h.screenshots
        let (result, _) = try await h.run()
        #expect(result.outcome.status == .success)
        let image = try #require(h.model.requests.last?.image)
        #expect(screenshots.screenshot(for: image.id)?.mimeType == "image/jpeg")
    }

    @Test("the effect cap stops the run and still saves memory")
    func effectCap() async throws {
        var h = Harness()
        h.configuration.maxEffects = 3
        let (result, _) = try await h.run()
        #expect(result.endReason == .effectLimitReached(3))
        #expect(result.outcome.status == .cancelled)
        #expect(h.memory.saves.count == 1)
    }

    @Test("a failing save is reported, not thrown")
    func saveFailure() async throws {
        var h = Harness()
        h.memory = RecordingMemory(failing: true)
        let (result, updates) = try await h.run()
        #expect(result.outcome.status == .success)
        #expect(result.memory == .saveFailed("disk full"))
        #expect(h.memory.saves.count == 1)
        #expect(updates.contains(.ioFailed(.saveMemory, message: "disk full", willRetry: false)))
    }
}
