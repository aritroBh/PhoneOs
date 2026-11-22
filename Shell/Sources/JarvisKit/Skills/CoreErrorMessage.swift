import Foundation

/// The case name and message of an error thrown by the generated bindings.
///
/// The generated `JarvisError` is `internal` to `JarvisCore`, so Swift outside it cannot
/// name or match it. Its cases all carry one `message: String`; `Mirror` reads the case
/// name and that message without depending on the type, so what the core said reaches the
/// owner verbatim (no `JarvisCore.JarvisError.…(message: "…")` wrapping, no escaping).
public struct CoreErrorMessage: Sendable, Equatable, CustomStringConvertible {
    /// The case name (`InvalidSkill`, `InvalidMemory`, …), `nil` when the error is not an
    /// enum case with a message.
    public var kind: String?
    /// The core's message, or `String(describing:)` of anything else.
    public var message: String

    public init(kind: String?, message: String) {
        self.kind = kind
        self.message = message
    }

    public init(_ error: any Error) {
        let mirror = Mirror(reflecting: error)
        if mirror.displayStyle == .enum, let child = mirror.children.first, let label = child.label {
            if let text = child.value as? String {
                self.init(kind: label, message: text)
                return
            }
            // A labelled single payload is reflected as a one-element tuple `(message: String)`.
            if let text = Mirror(reflecting: child.value).children.first?.value as? String {
                self.init(kind: label, message: text)
                return
            }
        }
        self.init(kind: nil, message: String(describing: error))
    }

    /// Whether the core refused a skill, a skill snapshot or a skill lookup.
    public var isInvalidSkill: Bool { kind == "InvalidSkill" }

    public var description: String { message }
}
