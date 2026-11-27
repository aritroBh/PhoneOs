import Foundation
import JarvisCore
import Synchronization
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Whether chat requests ask the server to keep the prompt prefix in its KV cache.
/// The system prompt is byte-identical at every step and is sent first, so a server that
/// caches the prefix evaluates only the user part per step.
public enum PromptCaching: String, Codable, Sendable, CaseIterable {
    /// Send `"cache_prompt": true`; when the server rejects the field (HTTP 400/422 naming
    /// it), retry that request once without it and stop sending it from this client on.
    case automatic
    /// Always send `"cache_prompt": true`; a server error is reported as it is.
    case on
    /// Never send the field.
    case off
}

/// A model client that can evaluate the fixed system prompt ahead of the first step.
/// Separate from `ModelProvider` so fakes need not implement it.
public protocol PromptWarming: Sendable {
    /// Best-effort prefix warm-up: never throws, never blocks other requests, result
    /// ignored. Once per distinct `system` per client unless `force`.
    func warmUp(system: String, force: Bool) async
}

extension PromptWarming {
    /// `warmUp(system:force:)` without forcing.
    public func warmUp(system: String) async { await warmUp(system: system, force: false) }
}

/// Starts `model`'s warm-up (when it has one) without awaiting it: a detached task whose
/// outcome the caller never sees. Call at task start; it cannot delay or alter the run.
public func startPromptWarmUp(for model: any ModelProvider, system: String) {
    guard let warming = model as? any PromptWarming else { return }
    Task.detached(priority: .utility) { await warming.warmUp(system: system, force: false) }
}

/// `ModelProvider` over an OpenAI-compatible `/v1/chat/completions` endpoint (llama.cpp
/// server, LM Studio, Ollama, vLLM, hosted APIs). The development backend and the
/// reference for on-device backends.
///
/// Request: a system and a user message (the user message becomes text + `image_url` data
/// URL parts when the request has a screenshot), `max_tokens`, `temperature: 0`, and when
/// the core sends a JSON schema and `constrainedDecoding == .jsonSchema`, a
/// `response_format` of type `json_schema`. With `.gbnfGrammar` the GBNF goes in a
/// top-level `grammar` field (a llama.cpp server extension).
public final class OpenAICompatibleModel: ModelProvider, PromptWarming {
    /// Server and model settings.
    public struct Configuration: Sendable {
        /// Server base URL, e.g. `http://127.0.0.1:8080` (the path below is appended).
        public var baseURL: URL
        /// Path of the endpoint.
        public var path: String
        /// The `model` field.
        public var model: String
        /// Sent as `Authorization: Bearer …` when set.
        public var apiKey: String?
        /// HTTP timeout in seconds.
        public var timeout: TimeInterval
        /// Sampling temperature (0 for reproducible actions).
        public var temperature: Double
        /// What the model can do; `constrainedDecoding` selects `response_format` or
        /// `grammar`.
        public var capabilities: ModelCapabilities
        /// Name given to the schema in `response_format.json_schema.name`.
        public var schemaName: String
        /// `response_format.json_schema.strict`.
        public var strictSchema: Bool
        /// Whether requests carry `"cache_prompt": true` (llama.cpp's prompt-prefix KV
        /// cache). See `PromptCaching`.
        public var promptCaching: PromptCaching

        /// Settings with development defaults.
        public init(
            baseURL: URL,
            path: String = "/v1/chat/completions",
            model: String,
            apiKey: String? = nil,
            timeout: TimeInterval = 120,
            temperature: Double = 0,
            capabilities: ModelCapabilities = ModelCapabilities(contextTokens: 8_192, supportsVision: false, constrainedDecoding: .jsonSchema),
            schemaName: String = "jarvis_action",
            strictSchema: Bool = false,
            promptCaching: PromptCaching = .automatic
        ) {
            self.baseURL = baseURL
            self.path = path
            self.model = model
            self.apiKey = apiKey
            self.timeout = timeout
            self.temperature = temperature
            self.capabilities = capabilities
            self.schemaName = schemaName
            self.strictSchema = strictSchema
            self.promptCaching = promptCaching
        }
    }

    public var capabilities: ModelCapabilities { configuration.capabilities }

    private let configuration: Configuration
    private let screenshots: ScreenshotStore
    private let annotator: any ScreenshotAnnotator
    private let transport: any HTTPTransport

    private struct CacheState {
        /// The server rejected `cache_prompt` (automatic mode only).
        var unsupported = false
        var warmInFlight = false
        var warmedPrompts: Set<Int> = []
    }
    private let cacheState = Mutex(CacheState())

    /// Seconds a warm-up may take.
    static let warmUpTimeout: TimeInterval = 5

    /// A client. `screenshots` must be the store the `SessionDriver` writes to.
    public init(
        configuration: Configuration,
        screenshots: ScreenshotStore,
        annotator: any ScreenshotAnnotator = NoAnnotation(),
        transport: (any HTTPTransport)? = nil
    ) {
        self.configuration = configuration
        self.screenshots = screenshots
        self.annotator = annotator
        self.transport = transport ?? URLSessionTransport(timeout: configuration.timeout)
    }

    public func generate(_ request: ModelRequest) async throws -> String {
        let body = try await requestBody(for: request)
        do {
            return try await post(body, timeout: configuration.timeout)
        } catch let ModelError.httpStatus(code, text)
            where configuration.promptCaching == .automatic && body.hasCachePrompt
                && (code == 400 || code == 422) && Self.mentionsCacheField(text) {
            // The server does not know `cache_prompt`: remember, and retry once without it.
            // Timeouts: the rejected call was an answer, not a timeout, and the retry is a
            // fresh HTTP call with the full `configuration.timeout`; the caller's own
            // deadline (the driver's per-call timeout) covers `generate` as a whole, so the
            // retry adds one HTTP call and no extra attempt in the caller's accounting.
            cacheState.withLock { $0.unsupported = true }
            return try await post(body.removingCachePrompt(), timeout: configuration.timeout)
        }
    }

    /// Whether an error body names the rejected field (conservative on purpose).
    static func mentionsCacheField(_ text: String) -> Bool {
        let t = text.lowercased()
        return ["cache_prompt", "unrecognized", "unknown field", "extra fields", "additional properties"]
            .contains { t.contains($0) }
    }

    /// Whether the next request carries `cache_prompt`.
    private var sendsCachePrompt: Bool {
        switch configuration.promptCaching {
        case .on: true
        case .off: false
        case .automatic: !cacheState.withLock { $0.unsupported }
        }
    }

    private func post(_ body: JSONValue, timeout: TimeInterval) async throws -> String {
        var http = URLRequest(url: configuration.baseURL.appendingPathComponent(String(configuration.path.drop(while: { $0 == "/" }))))
        http.httpMethod = "POST"
        http.timeoutInterval = timeout
        http.setValue("application/json", forHTTPHeaderField: "Content-Type")
        http.setValue("application/json", forHTTPHeaderField: "Accept")
        if let key = configuration.apiKey {
            http.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        }
        do {
            http.httpBody = try body.encoded()
        } catch {
            throw ModelError.malformedResponse("request not encodable: \(error)")
        }
        let data: Data
        let response: HTTPURLResponse
        do {
            (data, response) = try await transport.send(http)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw ModelError.transport(String(describing: error))
        }
        guard (200..<300).contains(response.statusCode) else {
            throw ModelError.httpStatus(response.statusCode, body: data.snippet())
        }
        return try Self.replyText(data)
    }

    public func warmUp(system: String, force: Bool) async {
        await warmUp(system: system, force: force, timeout: Self.warmUpTimeout)
    }

    /// `timeout` is a parameter for tests.
    func warmUp(system: String, force: Bool, timeout: TimeInterval) async {
        let key = system.hashValue
        let go = cacheState.withLock { s -> Bool in
            if s.warmInFlight || (!force && s.warmedPrompts.contains(key)) { return false }
            s.warmInFlight = true
            s.warmedPrompts.insert(key)
            return true
        }
        guard go else { return }
        var body: [String: JSONValue] = [
            "model": .string(configuration.model),
            "messages": [
                ["role": "system", "content": .string(system)],
                ["role": "user", "content": "ok"],
            ],
            "max_tokens": 1,
            "temperature": 0,
            "stream": false,
        ]
        if sendsCachePrompt { body["cache_prompt"] = true }
        let request = JSONValue.object(body)
        // Bounded by its own deadline even when the transport ignores cancellation: the
        // first of {reply, deadline} resumes the waiter and the other is cancelled.
        await Deadline.race(seconds: timeout) { [self] in
            do {
                _ = try await post(request, timeout: timeout)
            } catch ModelError.httpStatus(let code, let text)
                where configuration.promptCaching == .automatic && (code == 400 || code == 422) && Self.mentionsCacheField(text) {
                cacheState.withLock { $0.unsupported = true }
            } catch {}
        }
        cacheState.withLock { $0.warmInFlight = false }
    }

    /// The chat-completions body for `request` (internal for tests).
    func requestBody(for request: ModelRequest) async throws -> JSONValue {
        var userContent: JSONValue = .string(request.user)
        if let image = request.image {
            guard configuration.capabilities.supportsVision else { throw ModelError.visionUnsupported }
            guard let stored = screenshots.screenshot(for: image.id) else { throw ModelError.imageNotFound(image.id) }
            let annotated = request.marks.isEmpty
                ? stored
                : try await annotator.annotate(stored, marks: request.marks)
            let url = "data:\(annotated.mimeType);base64,\(annotated.data.base64EncodedString())"
            userContent = [
                ["type": "text", "text": .string(request.user)],
                ["type": "image_url", "image_url": ["url": .string(url)]],
            ]
        }
        var body: [String: JSONValue] = [
            "model": .string(configuration.model),
            "messages": [
                ["role": "system", "content": .string(request.system)],
                ["role": "user", "content": userContent],
            ],
            "max_tokens": .number(Double(request.maxTokens)),
            "temperature": .number(configuration.temperature),
            "stream": false,
        ]
        // The messages above keep the system prompt first, byte-for-byte as given, so the
        // server's prefix cache matches at every step.
        if sendsCachePrompt { body["cache_prompt"] = true }
        switch configuration.capabilities.constrainedDecoding {
        case .jsonSchema:
            if let schemaText = request.jsonSchema {
                let schema: JSONValue
                do {
                    schema = try JSONValue(parsing: schemaText)
                } catch {
                    throw ModelError.invalidSchema(String(describing: error))
                }
                body["response_format"] = [
                    "type": "json_schema",
                    "json_schema": [
                        "name": .string(configuration.schemaName),
                        "schema": schema,
                        "strict": .bool(configuration.strictSchema),
                    ],
                ]
            }
        case .gbnfGrammar:
            if let grammar = request.grammar {
                body["grammar"] = .string(grammar)
            }
        case .none:
            break
        }
        return .object(body)
    }

    /// `choices[0].message.content` of a response body.
    static func replyText(_ data: Data) throws -> String {
        guard let json = try? JSONValue(parsing: data), case .object = json else {
            throw ModelError.malformedResponse("not a JSON object: \(data.snippet(120))")
        }
        if let error = json["error"], error != .null {
            throw ModelError.server(error["message"]?.stringValue ?? error.stringValue ?? "unspecified")
        }
        guard let choices = json["choices"]?.arrayValue else {
            throw ModelError.malformedResponse("no choices array")
        }
        guard let first = choices.first else {
            throw ModelError.malformedResponse("empty choices")
        }
        guard let content = first["message"]?["content"] else {
            throw ModelError.malformedResponse("choices[0].message.content missing")
        }
        switch content {
        case .string(let text):
            return text
        case .array(let parts):
            // Some servers return content parts; join their text.
            let texts = parts.compactMap { $0["text"]?.stringValue }
            guard !texts.isEmpty else { throw ModelError.malformedResponse("content parts without text") }
            return texts.joined()
        default:
            throw ModelError.malformedResponse("content is \(content.typeName)")
        }
    }
}

extension JSONValue {
    fileprivate var hasCachePrompt: Bool {
        if case .object(let o) = self { return o["cache_prompt"] != nil }
        return false
    }

    fileprivate func removingCachePrompt() -> JSONValue {
        guard case .object(var o) = self else { return self }
        o["cache_prompt"] = nil
        return .object(o)
    }
}

/// Runs an operation against a deadline and returns at whichever comes first, even when the
/// operation ignores cancellation (it then finishes detached and its result is dropped).
///
/// Through `IORace`, the driver's own race, which compiles under Swift 6.3 on the Mac: a
/// first version with a local `Mutex` captured by two `Task` closures built with Swift 6.4
/// on Linux and failed on the Mac ("passing closure as a 'sending' parameter risks causing
/// data races").
enum Deadline {
    static func race(seconds: TimeInterval, _ operation: @escaping @Sendable () async -> Void) async {
        _ = try? await IORace<Void>().run(kind: .infer, timeout: .seconds(seconds)) {
            await operation()
        }
    }
}
