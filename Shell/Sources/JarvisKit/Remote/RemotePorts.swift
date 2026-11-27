import Foundation
import Synchronization

/// A value that must never be printed by accident: the pairing code, its QR payload,
/// credentials. `print`, string interpolation, `debugPrint` and `dump` show `<redacted>`;
/// only `reveal()` gives the value (to draw it on screen or hand it to the core).
public struct Redacted<Value: Sendable>: Sendable, CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
    private let value: Value

    public init(_ value: Value) {
        self.value = value
    }

    public func reveal() -> Value { value }

    public var description: String { "<redacted>" }
    public var debugDescription: String { "<redacted>" }
    public var customMirror: Mirror { Mirror(self, children: [], displayStyle: .struct) }
}

extension Redacted: Equatable where Value: Equatable {}

/// Where remote control keeps the hub's state (every device key). The app's adapter is
/// the Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable):
/// readable while the phone is locked after the first unlock, never in `UserDefaults`, a
/// file or a log.
public protocol SecretStore: Sendable {
    /// The stored value, `nil` when there is none. Throws when the store cannot be read
    /// (for example before the first unlock).
    func read(_ key: String) throws -> String?
    func write(_ key: String, _ value: String) throws
    func delete(_ key: String) throws
}

/// Fresh random bytes from the OS CSPRNG (the app: `SecRandomCopyBytes`). A failure is
/// fatal for the operation that asked; there is no fallback source.
public protocol RandomSource: Sendable {
    func bytes(count: Int) throws -> Data
}

/// What the local listener reports, in order.
public enum RemoteTransportEvent: Sendable, Equatable {
    /// Accepting connections (`detail`: e.g. the advertised service, for the UI).
    case listening(detail: String?)
    /// The listener stopped with an error.
    case failed(String)
    /// A connection opened; `id` is unique while it is open.
    case opened(UInt64)
    /// Bytes arrived (any chunking: framing is the hub's job).
    case received(UInt64, Data)
    /// The connection closed (by the peer or the network).
    case closed(UInt64)
}

/// The local-network listener. The app's adapter is an `NWListener` on TCP advertised over
/// Bonjour; tests and the simulator use `InMemoryRemoteTransport`.
public protocol RemoteTransport: Sendable {
    /// Starts listening. Events arrive on the stream in order; it ends after `stop()`.
    func start() -> AsyncStream<RemoteTransportEvent>
    func send(_ bytes: Data, to connection: UInt64)
    /// The consumer has taken `count` bytes of `.received` on `connection` (called once per
    /// `.received` event, with its byte count). The network transport counts received bytes
    /// not consumed yet and closes a connection that goes over its budget.
    func consumed(_ count: Int, on connection: UInt64)
    /// Closes one connection (no `.closed` event is required for it).
    func close(_ connection: UInt64)
    /// Stops listening and closes every connection.
    func stop()
    /// Sends like `send(_:to:)` and calls `written` with the byte count once the transport
    /// has handed the bytes to the network (`NWConnection`'s `.contentProcessed`). The hub
    /// gives the Live view a connection only while little is unconfirmed, so a peer that
    /// stops reading never makes the phone queue more. Never called for bytes of a
    /// connection that closed first. The default sends and confirms at once (a transport
    /// without a completion signal: no backpressure beyond the hub's own queue bound).
    func send(_ bytes: Data, to connection: UInt64, written: @escaping @Sendable (Int) -> Void)
    /// Where a controller can reach this phone while it listens (shown on the Remote
    /// control screen as the connection details): its local addresses and the port. Empty
    /// while not listening or when unknown (the default).
    func localAddresses() -> [RemotePhoneAddress]
}

extension RemoteTransport {
    public func localAddresses() -> [RemotePhoneAddress] { [] }

    public func send(_ bytes: Data, to connection: UInt64, written: @escaping @Sendable (Int) -> Void) {
        send(bytes, to: connection)
        written(bytes.count)
    }
}

/// A `SecretStore` in memory (tests, the simulator's QA mode).
public final class InMemorySecretStore: SecretStore {
    private let values = Mutex<[String: String]>([:])

    public init(_ initial: [String: String] = [:]) {
        values.withLock { $0 = initial }
    }

    public func read(_ key: String) throws -> String? { values.withLock { $0[key] } }
    public func write(_ key: String, _ value: String) throws { values.withLock { $0[key] = value } }
    public func delete(_ key: String) throws { _ = values.withLock { $0.removeValue(forKey: key) } }
}

/// Random bytes from `SystemRandomNumberGenerator` (a CSPRNG on Apple platforms and Linux).
/// For tests and the in-process QA client only; the app's hub uses `SecRandomCopyBytes`.
public struct GeneratorRandomSource: RandomSource {
    public init() {}

    public func bytes(count: Int) throws -> Data {
        var generator = SystemRandomNumberGenerator()
        return Data((0..<max(0, count)).map { _ in UInt8.random(in: .min ... .max, using: &generator) })
    }
}
