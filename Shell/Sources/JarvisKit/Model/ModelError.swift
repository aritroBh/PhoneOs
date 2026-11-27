import Foundation

/// Failures of a model backend (reported to the core as `Event.modelFailed`).
public enum ModelError: Error, Equatable, Sendable, CustomStringConvertible {
    /// The server could not be reached.
    case transport(String)
    /// A non-2xx HTTP status; `body` is the start of the response body.
    case httpStatus(Int, body: String)
    /// The body is not the expected response shape.
    case malformedResponse(String)
    /// The server answered with an `error` object.
    case server(String)
    /// The request has an image but this backend has no vision.
    case visionUnsupported
    /// The request's image is not (or no longer) in the screenshot store.
    case imageNotFound(String)
    /// The request's JSON schema is not valid JSON.
    case invalidSchema(String)

    public var description: String {
        switch self {
        case .transport(let message): "model server unreachable: \(message)"
        case .httpStatus(let code, let body): "model server HTTP \(code): \(body)"
        case .malformedResponse(let why): "model reply malformed: \(why)"
        case .server(let message): "model server error: \(message)"
        case .visionUnsupported: "this model backend has no vision support"
        case .imageNotFound(let id): "screenshot \(id) is not in the store"
        case .invalidSchema(let why): "request JSON schema invalid: \(why)"
        }
    }
}
