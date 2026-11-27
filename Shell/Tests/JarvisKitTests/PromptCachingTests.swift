import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

private let okReply = StubHTTPServer.Response.json([
    "choices": [["index": 0, "message": ["role": "assistant", "content": "{}"], "finish_reason": "stop"]],
])

private let systemText = "SYSTEM \u{1F916} line1\nline2  \ttrailing "

private func req(system: String = systemText) -> ModelRequest {
    ModelRequest(system: system, user: "USER", image: nil, marks: [], maxTokens: 160,
                 grammar: nil, jsonSchema: nil, kind: .text)
}

private func client(_ server: StubHTTPServer, _ caching: PromptCaching, timeout: TimeInterval = 5) -> OpenAICompatibleModel {
    OpenAICompatibleModel(
        configuration: .init(baseURL: server.baseURL, model: "m", timeout: timeout, promptCaching: caching),
        screenshots: ScreenshotStore()
    )
}

@Suite("Prompt caching of OpenAICompatibleModel", .timeLimit(.minutes(1)))
struct PromptCachingTests {
    @Test("on and automatic send cache_prompt, off does not")
    func fieldPerMode() async throws {
        for (mode, expected) in [(PromptCaching.on, true), (.automatic, true), (.off, false)] {
            let server = try StubHTTPServer { _ in okReply }
            defer { server.stop() }
            _ = try await client(server, mode).generate(req())
            let body = try #require(server.requests.first).json()
            #expect(body["cache_prompt"] == (expected ? true : nil), "mode \(mode)")
        }
    }

    @Test("the system message comes first and is byte-identical")
    func systemFirst() async throws {
        let server = try StubHTTPServer { _ in okReply }
        defer { server.stop() }
        _ = try await client(server, .automatic).generate(req())
        let body = try #require(server.requests.first).json()
        let messages = try #require(body["messages"]?.arrayValue)
        #expect(messages.count == 2)
        #expect(messages[0]["role"] == "system")
        #expect(messages[0]["content"]?.stringValue.map { Array($0.utf8) } == Array(systemText.utf8))
        #expect(messages[1]["role"] == "user")
    }

    @Test("automatic: a 400 naming the field retries once without it and remembers")
    func automaticFallback() async throws {
        let server = try StubHTTPServer { r in
            ((try? r.json())?["cache_prompt"] != nil)
                ? .json(["error": ["message": "Unrecognized request argument: cache_prompt"]], status: 400)
                : okReply
        }
        defer { server.stop() }
        let model = client(server, .automatic)
        #expect(try await model.generate(req()) == "{}")
        #expect(server.requests.count == 2)
        #expect(try server.requests[0].json()["cache_prompt"] == true)
        #expect(try server.requests[1].json()["cache_prompt"] == nil)
        #expect(try server.requests[1].json()["messages"] == server.requests[0].json()["messages"])
        _ = try await model.generate(req())
        #expect(server.requests.count == 3, "later requests omit the field, no further retry")
        #expect(try server.requests[2].json()["cache_prompt"] == nil)
    }

    @Test("a 422 naming extra fields also retries")
    func fallback422() async throws {
        let server = try StubHTTPServer { r in
            ((try? r.json())?["cache_prompt"] != nil)
                ? .text("Extra fields not permitted", status: 422) : okReply
        }
        defer { server.stop() }
        #expect(try await client(server, .automatic).generate(req()) == "{}")
        #expect(server.requests.count == 2)
    }

    @Test("automatic: unrelated 400 and a 500 are not retried and surface unchanged")
    func noRetry() async throws {
        for (status, text) in [(400, "context length exceeded"), (500, "unrecognized internal failure")] {
            let server = try StubHTTPServer { _ in .text(text, status: status) }
            defer { server.stop() }
            await #expect(throws: ModelError.httpStatus(status, body: text)) {
                _ = try await client(server, .automatic).generate(req())
            }
            #expect(server.requests.count == 1)
        }
    }

    @Test("on: a rejection is returned as it is, without retry")
    func onNoRetry() async throws {
        let server = try StubHTTPServer { _ in .text("unknown field cache_prompt", status: 400) }
        defer { server.stop() }
        await #expect(throws: ModelError.httpStatus(400, body: "unknown field cache_prompt")) {
            _ = try await client(server, .on).generate(req())
        }
        #expect(server.requests.count == 1)
    }

    @Test("warm-up sends max_tokens 1, the system prompt first, and ignores a failure")
    func warmUpShape() async throws {
        let server = try StubHTTPServer { _ in .text("boom", status: 500) }
        defer { server.stop() }
        await client(server, .automatic).warmUp(system: systemText)
        let body = try #require(server.requests.first).json()
        #expect(server.requests.count == 1)
        #expect(body["max_tokens"] == 1)
        #expect(body["temperature"] == 0)
        #expect(body["cache_prompt"] == true)
        #expect(body["messages"]?.arrayValue?.first?["content"]?.stringValue == systemText)
        #expect(body["messages"]?.arrayValue?.last?["content"] == "ok")
    }

    @Test("warm-up runs once per system prompt unless forced")
    func warmUpOnce() async throws {
        let server = try StubHTTPServer { _ in okReply }
        defer { server.stop() }
        let model = client(server, .automatic)
        await model.warmUp(system: "A")
        await model.warmUp(system: "A")
        #expect(server.requests.count == 1)
        await model.warmUp(system: "B")
        #expect(server.requests.count == 2)
        await model.warmUp(system: "A", force: true)
        #expect(server.requests.count == 3)
    }

    @Test("warm-up times out without throwing")
    func warmUpTimeout() async throws {
        let server = try StubHTTPServer { _ in .json(["choices": []], delay: 3) }
        defer { server.stop() }
        let started = ContinuousClock.now
        await client(server, .automatic).warmUp(system: "A", force: false, timeout: 0.3)
        #expect(ContinuousClock.now - started < .seconds(2.5))
    }

    @Test("a real request is not delayed by a pending warm-up")
    func warmUpDoesNotBlock() async throws {
        let server = try StubHTTPServer { r in
            (try? r.json())?["max_tokens"] == 1 ? .json(["choices": []], delay: 2) : okReply
        }
        defer { server.stop() }
        let model = client(server, .automatic)
        let warm = Task { await model.warmUp(system: "A") }
        try await waitUntil { server.requests.count == 1 }
        let started = ContinuousClock.now
        #expect(try await model.generate(req()) == "{}")
        #expect(ContinuousClock.now - started < .seconds(1.5))
        await warm.value
    }

    @Test("at most one warm-up in flight")
    func oneInFlight() async throws {
        let server = try StubHTTPServer { _ in .json(["choices": []], delay: 1) }
        defer { server.stop() }
        let model = client(server, .automatic)
        async let first: Void = model.warmUp(system: "A")
        try await waitUntil { server.requests.count == 1 }
        await model.warmUp(system: "B", force: true)
        await first
        #expect(server.requests.count == 1)
    }

    @Test("settings default to automatic, round-trip, and survive a bad value")
    func settings() throws {
        #expect(JarvisSettings().modelPromptCaching == .automatic)
        var s = JarvisSettings()
        s.modelPromptCaching = .off
        let back = try JSONDecoder().decode(JarvisSettings.self, from: JSONEncoder().encode(s))
        #expect(back.modelPromptCaching == .off)
        let bad = try JSONDecoder().decode(JarvisSettings.self, from: Data(#"{"model_prompt_caching":"sometimes"}"#.utf8))
        #expect(bad.modelPromptCaching == .automatic)
    }
}

/// A model whose warm-up never ends.
private final class HangingWarmModel: ModelProvider, PromptWarming {
    let inner: FakeModel
    let latch = Latch()
    var capabilities: ModelCapabilities { inner.capabilities }
    init(_ inner: FakeModel) { self.inner = inner }
    func generate(_ request: ModelRequest) async throws -> String { try await inner.generate(request) }
    func warmUp(system: String, force: Bool) async { await latch.hang() }
}

@Suite("Warm-up never changes a run", .timeLimit(.minutes(1)))
struct WarmUpRunTests {
    private func commands(model: any ModelProvider, warm: Bool) async throws -> [RunnerCommand] {
        let h = Harness()
        let config = try SessionConfigBuilder.make(
            capabilities: model.capabilities, tools: await h.tools.toolSpecs(),
            knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")], adjustPolicy: { _ in }
        )
        if warm { startPromptWarmUp(for: model, system: "SYSTEM") }
        let deps = DriverDependencies(
            runner: h.phone, model: model, tools: h.tools, human: h.gate, memory: h.memory,
            screenshots: h.screenshots, clock: SteppingClock()
        )
        let driver = try SessionDriver(config: config, task: h.task, memoryJSON: "", dependencies: deps, configuration: h.configuration)
        let result = await driver.run()
        #expect(result.outcome.status == .success)
        return h.phone.commands
    }

    @Test("same commands with no warm-up and with a hanging one")
    func sameCommands() async throws {
        let plain = try await commands(model: FakeModel(), warm: false)
        let hanging = HangingWarmModel(FakeModel())
        let warmed = try await commands(model: hanging, warm: true)
        #expect(!plain.isEmpty)
        #expect(plain == warmed)
        try await waitUntil { hanging.latch.entered == 1 }
        hanging.latch.release()
    }
}
