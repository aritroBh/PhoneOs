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
///
/// The file follows the store rules (`StoredFileRead`): while it cannot be read (before the
/// first unlock, an I/O error) new entries are kept in memory only and the file is not
/// touched; every later call reads it again, and once it can be read the entries collected
/// meanwhile are merged with the file's (newest first, bounded) and written. A damaged file
/// is set aside and `loadIssue` says where.
public final class RemoteAuditLog: Sendable {
    public let fileURL: URL?
    public let capacity: Int
    private let now: @Sendable () -> Date

    private struct State {
        var records: [RemoteAuditRecord] = []
        /// The file has not been read yet (it could not be): entries are in memory only.
        var unread: Bool
        var issue: String?
    }

    private let state: Mutex<State>

    public init(fileURL: URL? = nil, capacity: Int = 300, now: @escaping @Sendable () -> Date = Date.init) {
        self.fileURL = fileURL
        self.capacity = max(1, capacity)
        self.now = now
        self.state = Mutex(State(unread: fileURL != nil))
        state.withLock { s in
            if syncIfUnread(&s) { persist(s.records) }
        }
    }

    /// The entries, newest first (reads the file again while it could not be read).
    public var entries: [RemoteAuditRecord] {
        state.withLock { s in
            if syncIfUnread(&s) { persist(s.records) }
            return s.records
        }
    }

    /// Why the log on disk could not be used as it was (unreadable now, or set aside).
    public var loadIssue: String? { state.withLock { $0.issue } }

    public func append(_ record: RemoteAuditRecord) {
        state.withLock { s in
            s.records.insert(record, at: 0)
            if s.records.count > capacity { s.records.removeLast(s.records.count - capacity) }
            _ = syncIfUnread(&s)
            if !s.unread { persist(s.records) }
        }
    }

    /// Empties the log. While the file cannot be read only the entries in memory go: what
    /// the file holds was not seen, so it is not removed.
    public func clear() {
        state.withLock { s in
            s.records = []
            _ = syncIfUnread(&s)
            guard !s.unread else { return }
            s.records = []
            if let fileURL { try? FileManager.default.removeItem(at: fileURL) }
        }
    }

    /// While the file has not been read: reads it and merges it with the entries collected
    /// in memory. Returns whether the merged list must be written.
    private func syncIfUnread(_ s: inout State) -> Bool {
        guard s.unread, let fileURL else { return false }
        let read = StoredFile.read(fileURL, now: now) { data in
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            return try decoder.decode([RemoteAuditRecord].self, from: data)
        }
        let onDisk: [RemoteAuditRecord]
        switch read {
        case .loaded(let records):
            onDisk = records
            s.issue = nil
        case .missing:
            onDisk = []
            s.issue = nil
        case .unreadable(let reason):
            s.issue = "The audit log could not be read (\(reason)); new entries are kept until it can be."
            return false
        case .damaged(let reason, let setAside):
            guard let setAside else {
                s.issue = "The audit log is damaged (\(reason)) and could not be set aside; it is not changed."
                return false
            }
            onDisk = []
            s.issue = "The audit log was damaged (\(reason)) and was kept as \(setAside.lastPathComponent); nothing was deleted."
        }
        let collected = s.records
        s.unread = false
        s.records = Self.merge(collected, onDisk, capacity: capacity)
        if case .damaged = read { return true }
        return !collected.isEmpty
    }

    /// `collected` (newer, kept in memory) and `onDisk`, newest first by time (the
    /// collected entry first on a tie), without duplicates, bounded.
    static func merge(_ collected: [RemoteAuditRecord], _ onDisk: [RemoteAuditRecord], capacity: Int) -> [RemoteAuditRecord] {
        var seen = Set<UUID>()
        let all = (collected.enumerated().map { (0, $0.offset, $0.element) }
            + onDisk.enumerated().map { (1, $0.offset, $0.element) })
            .sorted { a, b in
                if a.2.at != b.2.at { return a.2.at > b.2.at }
                if a.0 != b.0 { return a.0 < b.0 }
                return a.1 < b.1
            }
            .map(\.2)
            .filter { seen.insert($0.id).inserted }
        return Array(all.prefix(max(1, capacity)))
    }

    /// Writes `records` (called under the lock, only once the file has been read).
    private func persist(_ records: [RemoteAuditRecord]) {
        guard let fileURL else { return }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        guard let data = try? encoder.encode(records) else { return }
        try? StoredFile.write(data, to: fileURL)
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
            if let text = liveView(accepted: request, detail: detail) { (text, false) }
            else { ("Accepted \(request)" + (detail.map { ": \($0)" } ?? ""), false) }
        case .denied(let request, let reason):
            if liveViewRequests.contains(request) { ("Refused to show the screen layout: \(describe(reason))", true) }
            else { ("Refused \(request): \(describe(reason))", true) }
        case .rejected(let request, let code): ("Rejected \(request ?? "a message"): \(describe(code))", true)
        case .revoked: ("Removed", false)
        case .permissionsChanged(let p): ("Permissions: \(describe(p))", false)
        case .deviceConfirmed: ("Confirmed on this phone", false)
        case .pendingExpired: ("Removed: not confirmed within 10 minutes", true)
        }
    }

    /// The Live view stream's requests (`start_watching`, `stop_watching`, `viewer_resync`).
    static let liveViewRequests: Set<String> = ["start_watching", "stop_watching", "viewer_resync"]

    /// Words for the Live view stream's accepted requests and the permission's changes;
    /// `nil` for anything else.
    static func liveView(accepted request: String, detail: String?) -> String? {
        switch request {
        case "start_watching": return "Started watching the screen layout"
        case "stop_watching":
            return detail == "permission revoked"
                ? "Stopped watching the screen layout: the permission was turned off"
                : "Stopped watching the screen layout"
        case "watch_screen_permission":
            return "Watch the screen layout: \(detail == "on" ? "on" : "off")"
        default: return nil
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
