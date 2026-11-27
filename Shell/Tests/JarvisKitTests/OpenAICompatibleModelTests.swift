import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

private func completion(_ content: JSONValue) -> StubHTTPServer.Response {
    .json(["id": "x", "object": "chat.completion",
           "choices": [["index": 0, "message": ["role": "assistant", "content": content], "finish_reason": "stop"]]])
}

private func request(
    image: ImageRef? = nil,
    kind: RequestKind = .text,
    grammar: String? = nil,
    schema: String? = nil,
    marks: [Mark] = []
) -> ModelRequest {
    ModelRequest(system: "SYSTEM", user: "USER", image: image, marks: marks, maxTokens: 160,
                 grammar: grammar, jsonSchema: schema, kind: kind)
}

private func model(
    _ server: StubHTTPServer,
    store: ScreenshotStore = ScreenshotStore(),
    vision: Bool = false,
    decoding: ConstrainedDecoding = .jsonSchema,
    timeout: TimeInterval = 5,
    apiKey: String? = nil
) -> OpenAICompatibleModel {
    OpenAICompatibleModel(
        configuration: .init(
            baseURL: server.baseURL, model: "gemma-test", apiKey: apiKey, timeout: timeout,
            capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: vision, constrainedDecoding: decoding)
        ),
        screenshots: store
    )
}

@Suite("OpenAICompatibleModel against a stub server", .timeLimit(.minutes(1)))
struct OpenAICompatibleModelTests {
    @Test("text request shape with the core's JSON schema")
    func textRequest() async throws {
        let server = try StubHTTPServer { _ in completion(#"{"action":"done","summary":"ok"}"#) }
        defer { server.stop() }
        let schema = try actionJsonSchema()
        let reply = try await model(server, apiKey: "sk-test").generate(request(grammar: "root ::= x", schema: schema))
        #expect(reply == #"{"action":"done","summary":"ok"}"#)

        let sent = try #require(server.requests.first)
        #expect(sent.method == "POST")
        #expect(sent.path == "/v1/chat/completions")
        #expect(sent.headers["authorization"] == "Bearer sk-test")
        #expect(sent.headers["content-type"] == "application/json")
        let body = try sent.json()
        #expect(body["model"] == "gemma-test")
        #expect(body["max_tokens"] == 160)
        #expect(body["temperature"] == 0)
        #expect(body["stream"] == false)
        #expect(body["messages"] == [
            ["role": "system", "content": "SYSTEM"],
            ["role": "user", "content": "USER"],
        ])
        #expect(body["response_format"]?["type"] == "json_schema")
        #expect(body["response_format"]?["json_schema"]?["name"] == "jarvis_action")
        #expect(body["response_format"]?["json_schema"]?["schema"] == (try JSONValue(parsing: schema)))
        #expect(body["grammar"] == nil, "schema mode sends no grammar")
    }

    @Test("no schema in the request, no response_format")
    func noSchema() async throws {
        let server = try StubHTTPServer { _ in completion("x") }
        defer { server.stop() }
        _ = try await model(server).generate(request())
        let body = try #require(try server.requests.first?.json())
        #expect(body["response_format"] == nil)
        #expect(body["grammar"] == nil)
        #expect(server.requests.first?.headers["authorization"] == nil)
    }

    @Test("GBNF mode passes the grammar through")
    func grammarMode() async throws {
        let server = try StubHTTPServer { _ in completion("x") }
        defer { server.stop() }
        let gbnf = try actionGbnf()
        _ = try await model(server, decoding: .gbnfGrammar).generate(request(grammar: gbnf, schema: "{}"))
        let body = try #require(try server.requests.first?.json())
        #expect(body["grammar"] == .string(gbnf))
        #expect(body["response_format"] == nil)
    }

    @Test("vision request carries the stored screenshot as a data URL")
    func visionRequest() async throws {
        let server = try StubHTTPServer { _ in completion("x") }
        defer { server.stop() }
        let store = ScreenshotStore()
        let bytes = Data([0xFF, 0xD8, 0xFF, 0x01, 0x02])
        let ref = store.store(CapturedScreenshot(data: bytes, mimeType: "image/jpeg", pixelWidth: 10, pixelHeight: 20))
        _ = try await model(server, store: store, vision: true).generate(request(image: ref, kind: .vision))
        let body = try #require(try server.requests.first?.json())
        let user = try #require(body["messages"]?.arrayValue?.last)
        #expect(user["content"] == [
            ["type": "text", "text": "USER"],
            ["type": "image_url", "image_url": ["url": .string("data:image/jpeg;base64,\(bytes.base64EncodedString())")]],
        ])
    }

    @Test("missing image and no vision are typed errors, sent nowhere")
    func imageErrors() async throws {
        let server = try StubHTTPServer { _ in completion("x") }
        defer { server.stop() }
        let ghost = ImageRef(id: "shot-404", width: 1, height: 1)
        await #expect(throws: ModelError.imageNotFound("shot-404")) {
            _ = try await model(server, vision: true).generate(request(image: ghost, kind: .vision))
        }
        await #expect(throws: ModelError.visionUnsupported) {
            _ = try await model(server, vision: false).generate(request(image: ghost, kind: .vision))
        }
        #expect(server.requests.isEmpty)
    }

    @Test("non-2xx, error objects and malformed bodies are typed errors")
    func responseErrors() async throws {
        let cases: [(StubHTTPServer.Response, ModelError)] = [
            (.text("overloaded", status: 503), .httpStatus(503, body: "overloaded")),
            (.text("not json"), .malformedResponse("not a JSON object: not json")),
            (.json(["error": ["message": "model not loaded"]]), .server("model not loaded")),
            (.json(["choices": []]), .malformedResponse("empty choices")),
            (.json(["object": "x"]), .malformedResponse("no choices array")),
            (.json(["choices": [["message": ["role": "assistant"]]]]), .malformedResponse("choices[0].message.content missing")),
            (completion(.null), .malformedResponse("content is null")),
        ]
        for (response, expected) in cases {
            let server = try StubHTTPServer { _ in response }
            await #expect(throws: expected) {
                _ = try await model(server).generate(request())
            }
            server.stop()
        }
    }

    @Test("content parts are joined")
    func contentParts() async throws {
        let server = try StubHTTPServer { _ in completion([["type": "text", "text": "{\"a\":"], ["type": "text", "text": "1}"]]) }
        defer { server.stop() }
        #expect(try await model(server).generate(request()) == "{\"a\":1}")
    }

    @Test("an invalid schema is a typed error")
    func invalidSchema() async throws {
        let server = try StubHTTPServer { _ in completion("x") }
        defer { server.stop() }
        await #expect {
            _ = try await model(server).generate(request(schema: "{not json"))
        } throws: { error in
            if case ModelError.invalidSchema = error { return true }
            return false
        }
    }

    @Test("a server that does not answer times out")
    func timeout() async throws {
        let server = try StubHTTPServer { _ in .json(["choices": []], delay: 3) }
        defer { server.stop() }
        let started = ContinuousClock.now
        await #expect {
            _ = try await model(server, timeout: 0.3).generate(request())
        } throws: { error in
            if case ModelError.transport = error { return true }
            return false
        }
        #expect(ContinuousClock.now - started < .seconds(2.5))
    }

    @Test("capabilities fill the core's model profile from its defaults")
    func capabilitiesProfile() throws {
        let caps = ModelCapabilities(contextTokens: 32_000, supportsVision: true, constrainedDecoding: .gbnfGrammar)
        let profile = try caps.modelProfile()
        #expect(profile.contextTokens == 32_000)
        #expect(profile.supportsVision)
        #expect(profile.supportsGrammar)
        #expect(try ModelCapabilities(contextTokens: 1, supportsVision: false).modelProfile().supportsGrammar == false)
    }
}
