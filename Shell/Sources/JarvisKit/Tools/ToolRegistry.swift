import Foundation
import JarvisCore
import Synchronization

/// The handler of one tool: gets the parsed arguments, returns the output text for the
/// model, throws on failure (`ToolError` or anything else).
public typealias ToolHandler = @Sendable (ToolArguments) async throws -> String

/// A `ToolHost` holding registered tools in registration order.
///
/// `execute` never throws: an unknown tool, arguments that are not an object or lack a
/// required parameter, and handler errors all become `ToolResult(ok: false, …)`.
public final class ToolRegistry: ToolHost {
    private struct Entry {
        var spec: ToolSpec
        var handler: ToolHandler
    }

    private let entries = Mutex<[Entry]>([])

    /// An empty registry.
    public init() {}

    /// Registers (or replaces, by name) a tool.
    public func register(_ spec: ToolSpec, handler: @escaping ToolHandler) {
        entries.withLock { entries in
            entries.removeAll { $0.spec.name == spec.name }
            entries.append(Entry(spec: spec, handler: handler))
        }
    }

    /// Removes the tool named `name`, if registered.
    public func unregister(_ name: String) {
        entries.withLock { $0.removeAll { $0.spec.name == name } }
    }

    /// The registered specs, in registration order.
    public var specs: [ToolSpec] {
        entries.withLock { $0.map(\.spec) }
    }

    public func toolSpecs() async -> [ToolSpec] {
        specs
    }

    public func execute(_ call: ToolCall) async -> ToolResult {
        let entry = entries.withLock { $0.first { $0.spec.name == call.name } }
        guard let entry else {
            return Self.failure(.unknownTool(call.name))
        }
        let arguments: ToolArguments
        do {
            arguments = try ToolArguments(json: call.argsJson)
        } catch {
            return Self.failure(error)
        }
        let missing = entry.spec.params.filter { $0.required && arguments[$0.name] == nil }.map(\.name)
        if !missing.isEmpty {
            return Self.failure(.invalidArguments("missing required parameter(s): \(missing.joined(separator: ", "))"))
        }
        do {
            let output = try await entry.handler(arguments)
            return ToolResult(ok: true, output: output, error: nil)
        } catch let error as ToolError {
            return Self.failure(error)
        } catch {
            return Self.failure(.failed(String(describing: error)))
        }
    }

    private static func failure(_ error: ToolError) -> ToolResult {
        ToolResult(ok: false, output: "", error: error.description)
    }
}
