import Foundation
import JarvisCore

/// Executes the tools the agent may call (`Effect.callTool`).
///
/// `execute` cannot throw by design: every failure (unknown tool, bad arguments, a handler
/// error) is a `ToolResult` with `ok == false`, which the core shows to the model.
public protocol ToolHost: Sendable {
    /// The tools to offer the core (put into `SessionConfig.tools`).
    func toolSpecs() async -> [ToolSpec]

    /// Runs one call.
    func execute(_ call: ToolCall) async -> ToolResult
}

/// The arguments of one tool call: the JSON object the model produced.
public struct ToolArguments: Sendable, Equatable {
    /// The members of the arguments object.
    public let values: [String: JSONValue]

    /// Wraps already-parsed arguments.
    public init(_ values: [String: JSONValue]) {
        self.values = values
    }

    /// Parses `ToolCall.argsJson`; an empty string means no arguments.
    public init(json: String) throws(ToolError) {
        if json.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            self.values = [:]
            return
        }
        let parsed: JSONValue
        do {
            parsed = try JSONValue(parsing: json)
        } catch {
            throw .invalidArguments("arguments are not valid JSON")
        }
        guard let object = parsed.objectValue else {
            throw .invalidArguments("arguments must be a JSON object, got \(parsed.typeName)")
        }
        self.values = object
    }

    /// The argument `name`.
    public subscript(name: String) -> JSONValue? { values[name] }

    /// The argument `name` as a string (numbers and booleans are converted), or `nil`.
    public func string(_ name: String) -> String? {
        switch values[name] {
        case .string(let s): s
        case .number(let n): n.rounded() == n ? String(Int64(n)) : String(n)
        case .bool(let b): String(b)
        default: nil
        }
    }

    /// The argument `name` as a number (numeric strings are converted), or `nil`.
    public func number(_ name: String) -> Double? {
        switch values[name] {
        case .number(let n): n
        case .string(let s): Double(s)
        default: nil
        }
    }
}

/// Why a tool call failed; becomes `ToolResult.error`.
public enum ToolError: Error, Equatable, Sendable, CustomStringConvertible {
    /// No tool of that name is registered.
    case unknownTool(String)
    /// The arguments are not a JSON object or miss a required parameter.
    case invalidArguments(String)
    /// The handler failed.
    case failed(String)

    public var description: String {
        switch self {
        case .unknownTool(let name): "unknown tool '\(name)'"
        case .invalidArguments(let why): "invalid arguments: \(why)"
        case .failed(let why): why
        }
    }
}
