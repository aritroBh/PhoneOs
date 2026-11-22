import Foundation
import JarvisCore

/// A language model backend: the OpenAI-compatible HTTP client for development, and later
/// on-device runtimes (llama.cpp, LiteRT-LM, Apple Foundation Models).
///
/// Contract for implementations:
/// - `generate` returns the model's raw reply text; the core parses it (tolerantly).
/// - When `request.image` is set, resolve the pixels through the `ScreenshotStore` the
///   backend was created with (`store.screenshot(for: image.id)`); draw `request.marks`
///   if the runtime does not do it. A missing image is an error, not a text-only call.
/// - When `capabilities.constrainedDecoding` is not `.none`, the core sends both
///   `request.grammar` (GBNF) and `request.jsonSchema`; enforce whichever the runtime
///   supports.
/// - Honour task cancellation where the runtime can; `SessionDriver` stops waiting on
///   cancellation or timeout either way.
/// - Throw for any failure; the driver reports it to the core as `Event.modelFailed`.
public protocol ModelProvider: Sendable {
    /// What the backend can do; fills the core's `ModelProfile`.
    var capabilities: ModelCapabilities { get }

    /// Runs one model call and returns the reply text.
    func generate(_ request: ModelRequest) async throws -> String
}

/// Which constrained-decoding input a runtime can enforce.
public enum ConstrainedDecoding: Sendable, Equatable {
    /// Free-form generation; the core's tolerant parser does the rest.
    case none
    /// GBNF grammars (llama.cpp-style), from `ModelRequest.grammar`.
    case gbnfGrammar
    /// JSON schemas (`response_format`, Foundation Models guided generation), from
    /// `ModelRequest.jsonSchema`.
    case jsonSchema
}

/// What a model backend can do.
public struct ModelCapabilities: Sendable, Equatable {
    /// Context window in tokens (prompt plus reply).
    public var contextTokens: UInt32
    /// Whether the backend accepts a screenshot with a request.
    public var supportsVision: Bool
    /// Which constraint the backend enforces.
    public var constrainedDecoding: ConstrainedDecoding

    /// Creates a capability description.
    public init(contextTokens: UInt32, supportsVision: Bool, constrainedDecoding: ConstrainedDecoding = .none) {
        self.contextTokens = contextTokens
        self.supportsVision = supportsVision
        self.constrainedDecoding = constrainedDecoding
    }

    /// The core's `ModelProfile` for these capabilities. Starts from the core's
    /// `defaultModelProfile()` and sets only the fields known here, so profile fields
    /// added to the core later keep their defaults.
    public func modelProfile() throws -> ModelProfile {
        var profile = try defaultModelProfile()
        profile.contextTokens = contextTokens
        profile.supportsVision = supportsVision
        profile.supportsGrammar = constrainedDecoding != .none
        return profile
    }
}
