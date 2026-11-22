import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

@Suite("Support types")
struct SupportTests {
    @Test("JSONValue encodes integral numbers without a fraction and round-trips")
    func jsonValue() throws {
        let value: JSONValue = ["i": 3, "f": 0.5, "s": "a/b", "n": nil, "b": false, "a": [1, "x"]]
        let text = String(decoding: try value.encoded(), as: UTF8.self)
        #expect(text == #"{"a":[1,"x"],"b":false,"f":0.5,"i":3,"n":null,"s":"a/b"}"#)
        #expect(try JSONValue(parsing: text) == value)
    }

    @Test("SessionConfigBuilder starts from the core defaults")
    func configBuilder() throws {
        let defaults = try defaultSessionConfig()
        let config = try SessionConfigBuilder.make(
            capabilities: ModelCapabilities(contextTokens: 16_000, supportsVision: true),
            tools: [ToolSpecs.spec(name: "echo", description: "e")],
            knownApps: [AppInfo(bundleId: "a.b", name: "AB")]
        ) { $0.maxSteps = 7 }
        #expect(config.policy.maxSteps == 7)
        #expect(config.policy.deniedApps == defaults.policy.deniedApps)
        #expect(config.policy.confirm == defaults.policy.confirm)
        #expect(config.model.contextTokens == 16_000)
        #expect(config.tools.map(\.name) == ["echo"])
        #expect(config.knownApps.map(\.bundleId) == ["a.b"])
    }

    @Test("IORace: value, timeout, cancel")
    func ioRace() async throws {
        #expect(try await IORace<Int>().run(kind: .act, timeout: .seconds(5)) { 42 } == 42)
        await #expect(throws: DriverIOError.timedOut(.infer, .milliseconds(50))) {
            try await IORace<Int>().run(kind: .infer, timeout: .milliseconds(50)) {
                try await Task.sleep(for: .seconds(10))
                return 1
            }
        }
        let race = IORace<Int>()
        let latch = Latch()
        let task = Task { try await race.run(kind: .observe, timeout: nil) { await latch.hang(); return 1 } }
        try await waitUntil { latch.entered == 1 }
        race.cancel()
        await #expect(throws: CancellationError.self) { _ = try await task.value }
        latch.release()

        // Cancelled before it starts.
        let early = Task {
            try await Task.sleep(for: .seconds(10))
            return try await IORace<Int>().run(kind: .act, timeout: nil) { 1 }
        }
        early.cancel()
        await #expect(throws: CancellationError.self) { _ = try await early.value }
    }

    @Test("SystemMonotonicClock never goes backwards")
    func clock() async throws {
        let clock = SystemMonotonicClock()
        let a = clock.nowMs()
        try await Task.sleep(for: .milliseconds(20))
        let b = clock.nowMs()
        #expect(b >= a + 15)
    }
}
