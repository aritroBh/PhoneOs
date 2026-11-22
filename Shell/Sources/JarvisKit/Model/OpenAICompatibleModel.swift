import Foundation
import JarvisCore
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// `ModelProvider` over an OpenAI-compatible `/v1/chat/completions` endpoint (llama.cpp
/// server, LM Studio, Ollama, vLLM, hosted APIs). The development backend and the
/// reference for on-device backends.
///
/// Request: a system and a user message (the user message becomes text + `image_url` data
/// URL parts when the request has a screenshot), `max_tokens`, `temperature: 0`, and when
/// the core sends a JSON schema and `constrainedDecoding == .jsonSchema`, a
/// `response_format` of type `json_schema`. With `.gbnfGrammar` the GBNF goes in a
/// top-level `grammar` field (a llama.cpp server extension).
public final class OpenAICompatibleModel: ModelProvider {
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
            strictSchema: Bool = false
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
        }
    }

    public var capabilities: ModelCapabilities { configuration.capabilities }

    private let configuration: Configuration
    private let screenshots: ScreenshotStore
    private let annotator: any ScreenshotAnnotator
    private let transport: any HTTPTransport

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
        var http = URLRequest(url: configuration.baseURL.appendingPathComponent(String(configuration.path.drop(while: { $0 == "/" }))))
        http.httpMethod = "POST"
        http.timeoutInterval = configuration.timeout
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
