import Foundation
import JarvisCore

/// The case name and an owner-readable message of an error thrown by the generated
/// bindings, so what the core said reaches the owner verbatim (no
/// `JarvisCore.JarvisError.…(message: "…")` wrapping, no escaping).
///
/// `JarvisError` is `public` (the generator prints `public` and `enum JarvisError` on
/// separate lines); every case is matched here. Cases without a message get a fixed text
/// that never echoes input (the remote cases carry no key material by contract).
public struct CoreErrorMessage: Sendable, Equatable, CustomStringConvertible {
    /// The case name (`InvalidSkill`, `InvalidMemory`, …), `nil` when the error is not a
    /// `JarvisError`.
    public var kind: String?
    /// The core's message, or `String(describing:)` of anything else.
    public var message: String

    public init(kind: String?, message: String) {
        self.kind = kind
        self.message = message
    }

    public init(_ error: any Error) {
        guard let error = error as? JarvisError else {
            self.init(kind: nil, message: String(describing: error))
            return
        }
        switch error {
        case .InvalidMemory(let m): self.init(kind: "InvalidMemory", message: m)
        case .InvalidJson(let field, let m): self.init(kind: "InvalidJson", message: "\(field): \(m)")
        case .RunnerText(let m): self.init(kind: "RunnerText", message: m)
        case .Poisoned(let m): self.init(kind: "Poisoned", message: m)
        case .Internal(let m): self.init(kind: "Internal", message: m)
        case .InvalidSkill(let m): self.init(kind: "InvalidSkill", message: m)
        case .InvalidWorkflow(let m): self.init(kind: "InvalidWorkflow", message: m)
        case .WorkflowProtocol(let got, let expected):
            self.init(kind: "WorkflowProtocol", message: "the workflow run got \(got) while it waits for \(expected)")
        case .InvalidSchedule(let m): self.init(kind: "InvalidSchedule", message: m)
        case .RemoteEntropy(let problem):
            let what = switch problem {
            case .tooShort: "too short"
            case .notRandom: "not random"
            }
            self.init(kind: "RemoteEntropy", message: "the random bytes from the system were refused (\(what))")
        case .RemotePairingLocked:
            self.init(kind: "RemotePairingLocked", message: "pairing is locked for a while after failed attempts")
        case .RemoteUnknownDevice:
            self.init(kind: "RemoteUnknownDevice", message: "no paired device with this id")
        case .RemoteStoreInvalid(let m): self.init(kind: "RemoteStoreInvalid", message: m)
        case .RemoteLimitReached(let what, let max):
            self.init(kind: "RemoteLimitReached", message: "at most \(max) \(what): remove one first")
        case .RemoteInvalidName: self.init(kind: "RemoteInvalidName", message: "the name is empty")
        case .RemoteInvalidCode(let m): self.init(kind: "RemoteInvalidCode", message: m)
        case .RemoteAuthFailed: self.init(kind: "RemoteAuthFailed", message: "the connection failed authentication")
        case .RemoteNotConnected: self.init(kind: "RemoteNotConnected", message: "not connected")
        case .RemoteMessage(let m): self.init(kind: "RemoteMessage", message: m)
        }
    }

    /// Whether the core refused a skill, a skill snapshot or a skill lookup.
    public var isInvalidSkill: Bool { kind == "InvalidSkill" }

    /// Whether the core refused a workflow, a workflow snapshot or a workflow lookup.
    public var isInvalidWorkflow: Bool { kind == "InvalidWorkflow" }

    /// Whether the core rejected saved memory bytes (`JarvisSession` init, skill promotion).
    public var isInvalidMemory: Bool { kind == "InvalidMemory" }

    /// Whether the object that threw is unusable (a caught panic): rebuild it.
    public var isPoisoned: Bool { kind == "Poisoned" || kind == "Internal" }

    public var description: String { message }
}
