import Foundation
import JarvisCore
import Synchronization

/// One line of remote control's audit log.
public struct RemoteAuditRecord: Codable, Sendable, Equatable, Identifiable {
    public var id: UUID
    public var at: Date
    /// The device, by name when it is known.
    public var device: String?
    public var text: String
    /// A refusal, a failed handshake or a lockout (shown with the warning treatment).
    public var isWarning: Bool

    public init(id: UUID = UUID(), at: Date, device: String?, text: String, isWarning: Bool) {
        self.id = id
        self.at = at
        self.device = device
        self.text = text
        self.isWarning = isWarning
    }
}

/// The audit log: newest first, bounded, in a JSON file when `fileURL` is given. The core
/// keeps secrets out of audit entries; the shell adds only its own fixed texts and device
/// names.
public final class RemoteAuditLog: Sendable {
    public let fileURL: URL?
    public let capacity: Int
    private let records: Mutex<[RemoteAuditRecord]>

    public init(fileURL: URL? = nil, capacity: Int = 300) {
        self.fileURL = fileURL
        self.capacity = max(1, capacity)
        var loaded: [RemoteAuditRecord] = []
        if let fileURL, let data = try? Data(contentsOf: fileURL) {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            loaded = (try? decoder.decode([RemoteAuditRecord].self, from: data)) ?? []
        }
        self.records = Mutex(Array(loaded.prefix(max(1, capacity))))
    }

    public var entries: [RemoteAuditRecord] { records.withLock { $0 } }

    public func append(_ record: RemoteAuditRecord) {
        let snapshot = records.withLock { r -> [RemoteAuditRecord] in
            r.insert(record, at: 0)
            if r.count > capacity { r.removeLast(r.count - capacity) }
            return r
        }
        guard let fileURL else { return }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        guard let data = try? encoder.encode(snapshot) else { return }
        try? FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if canImport(Darwin)
        try? data.write(to: fileURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try? data.write(to: fileURL, options: [.atomic])
        #endif
    }

    public func clear() {
        records.withLock { $0 = [] }
        if let fileURL { try? FileManager.default.removeItem(at: fileURL) }
    }
}

/// Owner-facing words for the hub's audit events and reasons.
public enum RemoteAuditText {
    public static func describe(_ event: AuditEvent, wallTime: (UInt64) -> Date = { Date(timeIntervalSince1970: Double($0) / 1_000) }) -> (text: String, warning: Bool) {
        switch event {
        case .pairingExpired: ("A pairing code expired unused", false)
        case .paired(let name): ("Paired \u{201C}\(name)\u{201D}", false)
        case .pairingAbandoned: ("Pairing was cancelled", false)
        case .connected: ("Connected", false)
        case .closed(let reason): ("Disconnected: \(describe(reason))", reason != .peerClosed && reason != .idleTimeout)
        case .connectionRefused(let reason): ("Connection refused: \(describe(reason))", true)
        case .handshakeFailed(let failures):
            ("A connection failed to authenticate (\(failures) failed \(failures == 1 ? "attempt" : "attempts"))", true)
        case .preAuthSuppressed(let count, _):
            ("\(count) more failed connection \(count == 1 ? "attempt" : "attempts") (merged)", true)
        case .lockedOut(let untilMs):
            ("Too many failed attempts: locked until \(wallTime(untilMs).formatted(date: .omitted, time: .standard))", true)
        case .accepted(let request, let detail):
            ("Accepted \(request)" + (detail.map { ": \($0)" } ?? ""), false)
        case .denied(let request, let reason): ("Refused \(request): \(describe(reason))", true)
        case .rejected(let request, let code): ("Rejected \(request ?? "a message"): \(describe(code))", true)
        case .revoked: ("Removed", false)
        case .permissionsChanged(let p): ("Permissions: \(describe(p))", false)
        }
    }

    public static func describe(_ reason: CloseReason) -> String {
        switch reason {
        case .authFailed: "authentication failed"
        case .replayed: "a replayed message"
        case .lockedOut: "locked out after failed attempts"
        case .handshakeTimeout: "the handshake took too long"
        case .idleTimeout: "idle"
        case .tooManyConnections: "too many connections"
        case .duplicateConnection: "connected again elsewhere"
        case .tooLarge: "a message was too large"
        case .revoked: "the device was removed"
        case .tooManyDevices: "too many devices"
        case .badEntropy: "the phone's random source failed"
        case .peerClosed: "closed by the device"
        case .internal: "an internal error"
        }
    }

    public static func describe(_ reason: DenyReason) -> String {
        switch reason {
        case .notPermitted: "not allowed for this device"
        case .busy: "Jarvis is busy"
        case .notRunning: "nothing is running"
        case .staleRequest: "the question had changed"
        case .notRemotelyApprovable: "this can only be approved on the phone"
        case .unknownSkill: "not in the list remotes may run"
        }
    }

    public static func describe(_ code: RemoteErrorCode) -> String {
        switch code {
        case .malformed: "malformed"
        case .unknownMessage: "unknown message"
        case .rateLimited: "too many messages"
        case .invalidRequest: "invalid request"
        case .tooLarge: "too large"
        }
    }

    public static func describe(_ p: RemotePermissions) -> String {
        var on: [String] = []
        if p.startTasks { on.append("start and stop tasks") }
        if p.seeProgress { on.append("see progress") }
        if p.seeScreenText { on.append("see what Jarvis sees") }
        if p.approve { on.append("approve") }
        return on.isEmpty ? "nothing" : on.joined(separator: ", ")
    }
}
