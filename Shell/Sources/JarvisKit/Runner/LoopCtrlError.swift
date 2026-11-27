import Foundation

/// Failures of `LoopCtrlRunner`. Thrown from `snapshot`, and from `perform` for transport
/// and protocol problems (a command the runner refused is a `RunnerResult`, not a throw).
public enum LoopCtrlError: Error, Equatable, Sendable, CustomStringConvertible {
    /// The runner could not be reached (not running, wrong port, connection reset).
    case transport(String)
    /// An HTTP status other than 2xx without a JSON-RPC error body.
    case httpStatus(Int, body: String)
    /// The body is not a JSON-RPC response (or the sealer could not open it).
    case malformedResponse(method: String, reason: String)
    /// JSON-RPC error -32099: the pairing is missing or stale; pair again.
    case authenticationRequired(String)
    /// Another JSON-RPC error.
    case rpcError(method: String, code: Int, message: String)
    /// A field the adapter needs is absent from a result.
    case missingField(method: String, field: String)
    /// A field has a type or value the adapter does not understand (shape unverified).
    case unexpectedValue(method: String, field: String, found: String)

    public var description: String {
        switch self {
        case .transport(let message):
            "runner unreachable: \(message)"
        case .httpStatus(let code, let body):
            "runner HTTP \(code): \(body)"
        case .malformedResponse(let method, let reason):
            "runner \(method): malformed response: \(reason)"
        case .authenticationRequired(let message):
            "runner refused the request (pair Jarvis with the runner again): \(message)"
        case .rpcError(let method, let code, let message):
            "runner \(method) error \(code): \(message)"
        case .missingField(let method, let field):
            "runner \(method): result has no '\(field)'"
        case .unexpectedValue(let method, let field, let found):
            "runner \(method): unexpected '\(field)': \(found)"
        }
    }
}
