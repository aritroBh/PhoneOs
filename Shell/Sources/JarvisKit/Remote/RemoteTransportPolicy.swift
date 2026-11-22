import Foundation
import Synchronization

/// Limits of the app's network transport (`NetworkRemoteTransport`), in one place.
public enum RemoteTransportLimits {
    /// Mirrors `MAX_BUFFERED_BYTES` in `core/jarvis-remote/src/codec.rs`: the most unprocessed
    /// bytes the core buffers per connection, `4 + MAX_RECORD_LEN` where `MAX_RECORD_LEN =
    /// 2 + COUNTER_LEN (8) + MAX_PLAINTEXT_LEN (65 536) + TAG_LEN (16)`. Keep in step with it.
    public static let coreMaxBufferedBytes = 4 + 2 + 8 + 64 * 1_024 + 16
    /// Received bytes the hub has not consumed yet, per connection. Above this the
    /// connection is closed (a peer that sends faster than the hub reads cannot make the
    /// phone buffer without bound; nothing honest has more than one record in flight).
    public static let maxUnconsumedInboundBytes = 2 * coreMaxBufferedBytes
    /// Bytes handed to the network but not yet processed by it, per connection. Above this
    /// the connection is closed (a peer that stops reading).
    public static let maxUnsentOutboundBytes = 4 * coreMaxBufferedBytes
    /// A connection that has not become ready this long after it arrived is cancelled.
    public static let connectionReadyTimeoutMs = 10_000
    /// Connections the transport holds at once, ready or not. The hub serves at most 8
    /// (`MAX_CONNECTIONS` in `core/jarvis-remote/src/hub.rs`); the rest is room for ones
    /// still connecting. Above this a new connection is cancelled at once, so a host that
    /// opens connections in a loop cannot grow the table during the ready timeout.
    public static let maxConnections = 24
}

/// A thread-safe count of bytes in flight against a limit: `add` when bytes arrive (or are
/// sent), `remove` when they were consumed (or processed). The owner closes the connection
/// when `add` reports the budget is exceeded.
public final class ByteBudget: Sendable {
    public let limit: Int
    private let used = Mutex(0)

    public init(limit: Int) {
        self.limit = max(0, limit)
    }

    /// Adds `count` bytes; returns `false` when the total is now over the limit.
    @discardableResult
    public func add(_ count: Int) -> Bool {
        used.withLock { used in
            used += max(0, count)
            return used <= limit
        }
    }

    /// Removes `count` bytes (never below zero).
    public func remove(_ count: Int) {
        used.withLock { $0 = max(0, $0 - max(0, count)) }
    }

    public var outstanding: Int { used.withLock { $0 } }
    public var isOverBudget: Bool { outstanding > limit }
}

/// When to rebuild a listener that failed: the delay doubles with every failure in a row
/// (from `initialDelayMs`, capped at `maxDelayMs`) and starts again from the first step once
/// the listener was ready for `healthyAfterMs` before it failed. Pure: the caller passes the
/// time (any monotonic milliseconds).
public struct ListenerRestartPolicy: Sendable, Equatable {
    public let initialDelayMs: UInt64
    public let maxDelayMs: UInt64
    public let healthyAfterMs: UInt64
    /// Failures in a row (since the last healthy period).
    public private(set) var attempts = 0
    private var readySinceMs: UInt64?

    public init(initialDelayMs: UInt64 = 1_000, maxDelayMs: UInt64 = 60_000, healthyAfterMs: UInt64 = 30_000) {
        self.initialDelayMs = initialDelayMs
        self.maxDelayMs = max(initialDelayMs, maxDelayMs)
        self.healthyAfterMs = healthyAfterMs
    }

    /// The delay before rebuild number `attempt` (0-based).
    public func delayMs(forAttempt attempt: Int) -> UInt64 {
        var delay = initialDelayMs
        var step = 0
        while step < attempt && delay < maxDelayMs {
            delay = delay > maxDelayMs / 2 ? maxDelayMs : delay * 2
            step += 1
        }
        return min(delay, maxDelayMs)
    }

    /// The listener is ready (accepting connections).
    public mutating func listenerReady(nowMs: UInt64) {
        readySinceMs = nowMs
    }

    /// The listener failed: returns how long to wait before rebuilding it.
    public mutating func listenerFailed(nowMs: UInt64) -> UInt64 {
        if let since = readySinceMs, nowMs >= since, nowMs - since >= healthyAfterMs {
            attempts = 0
        }
        readySinceMs = nil
        let delay = delayMs(forAttempt: attempts)
        attempts += 1
        return delay
    }
}

/// Whether a peer may connect, judged on its address and the connection's path. The
/// pairing and the authenticated handshake are the real gate; this is defence in depth that
/// keeps the listener to the local network.
public enum PeerAddressDecision: Sendable, Equatable {
    case accept
    case refuse(String)
}

public enum PeerAddressPolicy {
    /// - Parameters:
    ///   - address: the peer's address in network byte order: 4 bytes (IPv4) or 16 (IPv6).
    ///   - pathIsCellular: whether the connection's path uses a cellular interface; `nil`
    ///     when it is not known (then only addresses that cannot be routed from outside the
    ///     local network are accepted).
    ///
    /// Accepted: loopback, link-local, private IPv4 (10/8, 172.16/12, 192.168/16,
    /// 169.254/16), unique-local IPv6 (fc00::/7), and global IPv6 (2000::/3, which home
    /// Wi-Fi hands out with SLAAC) only on a path known not to be cellular. IPv4-mapped IPv6
    /// follows the IPv4 rule. Refused: public IPv4, anything else, anything on a cellular path.
    public static func decide(address: [UInt8], pathIsCellular: Bool?) -> PeerAddressDecision {
        if pathIsCellular == true { return .refuse("the connection uses a cellular network") }
        switch address.count {
        case 4:
            return ipv4(address)
        case 16:
            if address[0..<10].allSatisfy({ $0 == 0 }) && address[10] == 0xFF && address[11] == 0xFF {
                return ipv4(Array(address[12..<16]))
            }
            if address[0..<15].allSatisfy({ $0 == 0 }) && address[15] == 1 { return .accept } // ::1
            if address[0] == 0xFE && address[1] & 0xC0 == 0x80 { return .accept } // fe80::/10
            if address[0] & 0xFE == 0xFC { return .accept } // fc00::/7
            if address[0] & 0xE0 == 0x20 { // 2000::/3
                return pathIsCellular == false ? .accept : .refuse("a global IPv6 address on a path that is not known to be local")
            }
            return .refuse("not a local IPv6 address")
        default:
            return .refuse("not an IP address")
        }
    }

    private static func ipv4(_ b: [UInt8]) -> PeerAddressDecision {
        let local = b[0] == 127 || b[0] == 10 || (b[0] == 172 && (16...31).contains(b[1])) || (b[0] == 192 && b[1] == 168)
            || (b[0] == 169 && b[1] == 254)
        return local ? .accept : .refuse("a public IPv4 address")
    }
}
