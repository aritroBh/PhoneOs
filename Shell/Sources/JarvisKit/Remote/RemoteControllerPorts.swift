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

        public var description: String {
            switch self {
            case .emptyHost: "Enter the phone's address."
            case .notAHost: "Enter only the address shown on the phone (no \u{201C}http://\u{201D}, slashes or spaces)."
            case .badPort: "The port is a number from 1 to 65535."
            case .notLocal(let why): "Only phones on this network can be controlled (\(why))."
            }
        }
    }

    /// Checks a typed address and port. Defence in depth only (the network check after the
    /// connection is up and the authenticated handshake are the real gates): an IPv4 literal
    /// must be a local address (`PeerAddressPolicy`); a host name is resolved and judged
    /// when the connection is ready.
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
        if let v4 = ipv4Bytes(host), case .refuse(let why) = PeerAddressPolicy.decide(address: v4, pathIsCellular: false) {
            return .failure(.notLocal(why))
        }
        return .success(.host(host, port: port))
    }

    /// The four bytes of a dotted IPv4 literal, `nil` for anything else.
    public static func ipv4Bytes(_ host: String) -> [UInt8]? {
        let parts = host.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return nil }
        var bytes: [UInt8] = []
        for part in parts {
            guard !part.isEmpty, part.count <= 3, part.allSatisfy(\.isNumber), let value = UInt8(part) else { return nil }
            bytes.append(value)
        }
        return bytes
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
/// then it stops and waits for the owner. Pure.
public struct ControllerReconnectPolicy: Sendable, Equatable {
    public var initialDelayMs: UInt64
    public var maxDelayMs: UInt64
    public var maxAttempts: Int
    /// Connections the phone closed before the handshake finished, in a row, after which
    /// the controller stops trying (the phone no longer accepts this device: it was removed,
    /// or it is locked out after failed attempts).
    public var maxRefusals: Int

    public init(initialDelayMs: UInt64 = 1_000, maxDelayMs: UInt64 = 30_000, maxAttempts: Int = 6, maxRefusals: Int = 2) {
        self.initialDelayMs = initialDelayMs
        self.maxDelayMs = max(initialDelayMs, maxDelayMs)
        self.maxAttempts = max(1, maxAttempts)
        self.maxRefusals = max(1, maxRefusals)
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
