import Foundation
import JarvisCore

// The controller's side of remote control: dialing a phone that runs Jarvis. The phone's
// side (`RemoteTransport`, a listener) is in `RemotePorts.swift`; both carry the same bytes
// (the core frames them), so a dialed connection is only a byte pipe.

/// Where a phone that runs Jarvis listens, as its Remote control screen shows it.
public enum RemotePhoneAddress: Sendable, Hashable, Codable, CustomStringConvertible {
    /// An address or host name on the local network and the listener's TCP port.
    case host(String, port: UInt16)
    /// A phone found over Bonjour (`_jarvis-remote._tcp`), by its service name.
    case service(name: String)
    /// The phone in this process (tests and the simulator's QA mode: no network).
    case inProcess

    public var description: String {
        switch self {
        case .host(let host, let port): host.contains(":") ? "[\(host)]:\(port)" : "\(host):\(port)"
        case .service(let name): "\(name) (found on this Wi-Fi)"
        case .inProcess: "this phone (in-process, no network)"
        }
    }

    /// What can be wrong with a typed address, in the owner's words.
    public enum Problem: Error, Sendable, Equatable, CustomStringConvertible {
        case emptyHost
        case notAHost
        case badPort
        case notLocal(String)
        /// A number that is not written the way the phone shows addresses (leading zeros,
        /// hex, fewer than four parts, a trailing dot): the system would read it as some
        /// other address, so it is refused rather than guessed.
        case notCanonical

        public var description: String {
            switch self {
            case .emptyHost: "Enter the phone's address."
            case .notAHost: "Enter only the address shown on the phone (no \u{201C}http://\u{201D}, slashes or spaces)."
            case .badPort: "The port is a number from 1 to 65535."
            case .notLocal(let why): "Only phones on this network can be controlled (\(why))."
            case .notCanonical: "Type the address exactly as the phone shows it: four numbers from 0 to 255 with dots (like 192.168.1.20), or the name ending in \u{201C}.local\u{201D}."
            }
        }
    }

    /// Checks a typed address and port. Defence in depth only (the network check after the
    /// connection is up and the authenticated handshake are the real gates), but it judges
    /// a literal the way the system resolver would read it, so nothing passes here as local
    /// that the system dials as a public host:
    /// - an IPv6 literal (it has a colon; brackets are optional) must parse and be local by
    ///   `PeerAddressPolicy` (link-local and unique-local allowed; an IPv4-mapped address is
    ///   judged by its IPv4 address);
    /// - a numeric-looking host (it starts with a digit, or its last label does) must be
    ///   canonical dotted-decimal IPv4 (exactly four parts, 0 to 255, no leading zeros, no
    ///   hex, no trailing dot) and a local address; every other numeric form (`010.8.8.8`,
    ///   `0x08080808`, `134744072`, `8.8.8.8.`) is refused;
    /// - anything else is a host name (for example `iphone.local`), resolved and judged when
    ///   the connection is ready.
    public static func parse(host rawHost: String, port rawPort: String) -> Result<RemotePhoneAddress, Problem> {
        var host = rawHost.trimmingCharacters(in: .whitespacesAndNewlines)
        if host.hasPrefix("["), host.hasSuffix("]") { host = String(host.dropFirst().dropLast()) }
        guard !host.isEmpty else { return .failure(.emptyHost) }
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: ".-:%"))
        guard host.unicodeScalars.allSatisfy({ allowed.contains($0) && $0.isASCII }), host.count <= 253 else {
            return .failure(.notAHost)
        }
        guard let port = UInt16(rawPort.trimmingCharacters(in: .whitespacesAndNewlines)), port > 0 else {
            return .failure(.badPort)
        }
        if host.contains(":") {
            guard let v6 = ipv6Bytes(host) else { return .failure(.notAHost) }
            if case .refuse(let why) = PeerAddressPolicy.decide(address: v6, pathIsCellular: false) {
                return .failure(.notLocal(why))
            }
            return .success(.host(host, port: port))
        }
        guard !host.contains("%") else { return .failure(.notAHost) }
        if looksNumeric(host) {
            guard let v4 = ipv4Bytes(host) else { return .failure(.notCanonical) }
            if case .refuse(let why) = PeerAddressPolicy.decide(address: v4, pathIsCellular: false) {
                return .failure(.notLocal(why))
            }
        }
        return .success(.host(host, port: port))
    }

    /// Whether the resolver could read `host` as a number: it starts with a digit, or its
    /// last label does (`inet_aton` forms, a trailing dot included).
    static func looksNumeric(_ host: String) -> Bool {
        if host.first?.isASCII == true, host.first?.isNumber == true { return true }
        let labels = host.split(separator: ".", omittingEmptySubsequences: true)
        guard let last = labels.last?.first else { return false }
        return last.isASCII && last.isNumber
    }

    /// The four bytes of a canonical dotted-decimal IPv4 literal (four parts, each 0 to 255
    /// written without leading zeros), `nil` for anything else.
    public static func ipv4Bytes(_ host: String) -> [UInt8]? {
        let parts = host.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return nil }
        var bytes: [UInt8] = []
        for part in parts {
            guard !part.isEmpty, part.count <= 3, part.allSatisfy({ $0.isASCII && $0.isNumber }),
                  part == "0" || part.first != "0", let value = UInt8(part) else { return nil }
            bytes.append(value)
        }
        return bytes
    }

    /// The sixteen bytes of an IPv6 literal (`::` compression, an embedded dotted IPv4 tail
    /// and a `%zone` suffix allowed), `nil` for anything else.
    public static func ipv6Bytes(_ raw: String) -> [UInt8]? {
        var text = raw
        if let pct = text.firstIndex(of: "%") {
            let zone = text[text.index(after: pct)...]
            guard !zone.isEmpty, zone.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber) }) else { return nil }
            text = String(text[..<pct])
        }
        var tail: [UInt8] = []
        if let lastColon = text.lastIndex(of: ":"), text[text.index(after: lastColon)...].contains(".") {
            guard let v4 = ipv4Bytes(String(text[text.index(after: lastColon)...])) else { return nil }
            tail = v4
            text = String(text[...lastColon])
            if !text.hasSuffix("::") { text = String(text.dropLast()) }
        }
        let needed = tail.isEmpty ? 8 : 6
        let halves = text.components(separatedBy: "::")
        guard halves.count <= 2 else { return nil }
        func groups(_ s: String) -> [UInt16]? {
            if s.isEmpty { return [] }
            var out: [UInt16] = []
            for part in s.split(separator: ":", omittingEmptySubsequences: false) {
                guard (1...4).contains(part.count), part.allSatisfy(\.isHexDigit), let v = UInt16(part, radix: 16) else { return nil }
                out.append(v)
            }
            return out
        }
        guard let head = groups(halves[0]) else { return nil }
        var all = head
        if halves.count == 2 {
            guard let rest = groups(halves[1]), head.count + rest.count < needed else { return nil }
            all = head + Array(repeating: 0, count: needed - head.count - rest.count) + rest
        } else {
            guard head.count == needed else { return nil }
        }
        return all.flatMap { [UInt8($0 >> 8), UInt8($0 & 0xFF)] } + tail
    }
}

/// One connection from the controller to a phone: bytes both ways, any chunking (framing is
/// the core's job).
public protocol RemoteConnection: Sendable {
    /// Bytes from the phone, in order; ends when either side closes.
    var incoming: AsyncStream<Data> { get }
    /// Sends bytes to the phone (nothing after `close`).
    func write(_ bytes: Data)
    func close()
}

/// Why a connection could not be opened.
public enum RemoteDialError: Error, Sendable, Equatable, CustomStringConvertible {
    /// Nothing answered at that address (or it did not become ready in time).
    case unreachable(String)
    /// It answered from an address or over a path remote control does not use (not local,
    /// or cellular).
    case refused(String)

    public var description: String {
        switch self {
        case .unreachable(let why): "The phone could not be reached (\(why))."
        case .refused(let why): "That connection is not on the local network (\(why))."
        }
    }
}

/// Opens connections to phones. The app's adapter is an `NWConnection` over TCP on the local
/// network only (never cellular, no relay), judged by `PeerAddressPolicy` once it is ready;
/// tests and the simulator's QA mode use `InMemoryRemoteDialer`.
public protocol RemoteDialer: Sendable {
    func dial(_ address: RemotePhoneAddress) async throws -> any RemoteConnection
}

extension InMemoryRemoteEndpoint: RemoteConnection {}

/// Dials the phone in this process over its `InMemoryRemoteTransport` (any address: there
/// is only one phone).
public struct InMemoryRemoteDialer: RemoteDialer {
    public let transport: InMemoryRemoteTransport

    public init(transport: InMemoryRemoteTransport) {
        self.transport = transport
    }

    public func dial(_ address: RemotePhoneAddress) async throws -> any RemoteConnection {
        guard let endpoint = transport.connect() else { throw RemoteDialError.unreachable("the phone is not listening") }
        return endpoint
    }
}

/// When the controller tries again after a connection went: the delay doubles from
/// `initialDelayMs` (capped at `maxDelayMs`) for at most `maxAttempts` attempts in a row;
/// then it stops and waits for the owner. A connection counts as having worked (and the
/// counts start again) only once it stayed up for `healthyAfterMs`, so a phone that accepts
/// and drops at once cannot keep the controller dialing. After `maxRefusals` refused
/// handshakes in a row the phone "did not accept this device" (it was removed there, or it
/// paused after failed attempts: a host on the network can cause the second, so this is
/// never read as "removed"); the controller then tries again every
/// `notAcceptedRetryDelayMs`, at most `maxNotAcceptedRetries` times until the owner's
/// Reconnect. Pure.
public struct ControllerReconnectPolicy: Sendable, Equatable {
    public var initialDelayMs: UInt64
    public var maxDelayMs: UInt64
    public var maxAttempts: Int
    /// Connections the phone closed before the handshake finished, in a row, after which
    /// the phone is taken not to accept this device for now.
    public var maxRefusals: Int
    /// A connection that stayed up this long resets the attempt and refusal counts.
    public var healthyAfterMs: UInt64
    /// The slow retry after the phone did not accept this device.
    public var notAcceptedRetryDelayMs: UInt64
    /// Slow retries before the controller waits for the owner (reset by Reconnect).
    public var maxNotAcceptedRetries: Int

    public init(initialDelayMs: UInt64 = 1_000, maxDelayMs: UInt64 = 30_000, maxAttempts: Int = 6, maxRefusals: Int = 2,
                healthyAfterMs: UInt64 = 30_000, notAcceptedRetryDelayMs: UInt64 = 60_000, maxNotAcceptedRetries: Int = 10) {
        self.initialDelayMs = initialDelayMs
        self.maxDelayMs = max(initialDelayMs, maxDelayMs)
        self.maxAttempts = max(1, maxAttempts)
        self.maxRefusals = max(1, maxRefusals)
        self.healthyAfterMs = healthyAfterMs
        self.notAcceptedRetryDelayMs = notAcceptedRetryDelayMs
        self.maxNotAcceptedRetries = max(0, maxNotAcceptedRetries)
    }

    /// The delay before retry number `attempt` (1-based); `nil` once the attempts are used up.
    public func delayMs(forAttempt attempt: Int) -> UInt64? {
        guard attempt >= 1, attempt <= maxAttempts else { return nil }
        var delay = initialDelayMs
        for _ in 1..<attempt where delay < maxDelayMs {
            delay = delay > maxDelayMs / 2 ? maxDelayMs : delay * 2
        }
        return min(delay, maxDelayMs)
    }
}
