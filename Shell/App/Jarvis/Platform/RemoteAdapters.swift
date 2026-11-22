import Foundation
import Network
import Security
import JarvisKit

/// Remote control's state in the Keychain: a generic password per key, readable after the
/// first unlock (so remote control works while the phone is locked), never synchronized,
/// never written anywhere else. Errors carry the OSStatus only, never the value.
struct KeychainSecretStore: SecretStore {
    struct KeychainError: Error, CustomStringConvertible {
        var status: OSStatus
        var description: String { "Keychain error \(status)" }
    }

    var service = "dev.jarvis.app.remote"

    private func base(_ key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecAttrSynchronizable as String: false,
        ]
    }

    func read(_ key: String) throws -> String? {
        var query = base(key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data, let text = String(data: data, encoding: .utf8) else {
            throw KeychainError(status: status)
        }
        return text
    }

    func write(_ key: String, _ value: String) throws {
        let data = Data(value.utf8)
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(base(key) as CFDictionary, attributes as CFDictionary)
        if status == errSecSuccess { return }
        guard status == errSecItemNotFound else { throw KeychainError(status: status) }
        var add = base(key)
        add.merge(attributes) { _, new in new }
        let added = SecItemAdd(add as CFDictionary, nil)
        guard added == errSecSuccess else { throw KeychainError(status: added) }
    }

    func delete(_ key: String) throws {
        let status = SecItemDelete(base(key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw KeychainError(status: status) }
    }
}

/// `SecRandomCopyBytes`. A failure is an error for the operation that asked; there is no
/// other source to fall back to.
struct SecureRandomSource: RandomSource {
    struct RandomError: Error, CustomStringConvertible {
        var status: OSStatus
        var description: String { "SecRandomCopyBytes failed (\(status))" }
    }

    func bytes(count: Int) throws -> Data {
        guard count > 0 else { return Data() }
        var data = Data(count: count)
        let status = data.withUnsafeMutableBytes { buffer -> OSStatus in
            guard let base = buffer.baseAddress else { return errSecAllocate }
            return SecRandomCopyBytes(kSecRandomDefault, count, base)
        }
        guard status == errSecSuccess else { throw RandomError(status: status) }
        return data
    }
}

/// The local listener: TCP on any free port, advertised over Bonjour as
/// `_jarvis-remote._tcp` (declared in `NSBonjourServices`). Peers outside the local network
/// (not private, link-local or loopback addresses) are refused; cellular is never used.
/// Bytes are handed to the hub as they arrive (framing is the hub's job).
final class NetworkRemoteTransport: RemoteTransport, @unchecked Sendable {
    static let serviceType = "_jarvis-remote._tcp"

    private let queue = DispatchQueue(label: "dev.jarvis.remote.transport")
    // Touched on `queue` only.
    private var listener: NWListener?
    private var connections: [UInt64: NWConnection] = [:]
    private var nextID: UInt64 = 1
    private var continuation: AsyncStream<RemoteTransportEvent>.Continuation?

    func start() -> AsyncStream<RemoteTransportEvent> {
        let (stream, continuation) = AsyncStream.makeStream(of: RemoteTransportEvent.self)
        queue.async { self.startListening(continuation) }
        return stream
    }

    func send(_ bytes: Data, to connection: UInt64) {
        queue.async {
            self.connections[connection]?.send(content: bytes, completion: .contentProcessed { _ in })
        }
    }

    func close(_ connection: UInt64) {
        queue.async {
            self.connections.removeValue(forKey: connection)?.cancel()
        }
    }

    func stop() {
        queue.async {
            self.listener?.cancel()
            self.listener = nil
            for connection in self.connections.values { connection.cancel() }
            self.connections = [:]
            self.continuation?.finish()
            self.continuation = nil
        }
    }

    // MARK: - On the queue

    private func startListening(_ continuation: AsyncStream<RemoteTransportEvent>.Continuation) {
        self.continuation = continuation
        let parameters = NWParameters.tcp
        parameters.includePeerToPeer = false
        parameters.prohibitedInterfaceTypes = [.cellular]
        do {
            let listener = try NWListener(using: parameters)
            listener.service = NWListener.Service(type: Self.serviceType)
            listener.stateUpdateHandler = { [weak self] state in self?.listenerChanged(state) }
            listener.newConnectionHandler = { [weak self] connection in self?.accept(connection) }
            listener.start(queue: queue)
            self.listener = listener
        } catch {
            continuation.yield(.failed("the listener could not start (\(error))"))
        }
    }

    private func listenerChanged(_ state: NWListener.State) {
        switch state {
        case .ready:
            let port = listener?.port.map { " on port \($0.rawValue)" } ?? ""
            continuation?.yield(.listening(detail: "Same Wi-Fi, \(Self.serviceType)\(port)"))
        case .failed(let error):
            continuation?.yield(.failed("the listener stopped (\(error))"))
        case .waiting(let error):
            continuation?.yield(.failed("waiting for the network (\(error))"))
        default:
            break
        }
    }

    private func accept(_ connection: NWConnection) {
        guard Self.isLocal(connection.endpoint) else {
            connection.cancel()
            return
        }
        let id = nextID
        nextID += 1
        connections[id] = connection
        connection.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready:
                self.continuation?.yield(.opened(id))
                self.receive(id, connection)
            case .failed, .cancelled:
                if self.connections.removeValue(forKey: id) != nil {
                    self.continuation?.yield(.closed(id))
                }
            default:
                break
            }
        }
        connection.start(queue: queue)
    }

    private func receive(_ id: UInt64, _ connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1_024) { [weak self] data, _, isComplete, error in
            guard let self else { return }
            if let data, !data.isEmpty, self.connections[id] != nil {
                self.continuation?.yield(.received(id, data))
            }
            if isComplete || error != nil {
                connection.cancel()
                return
            }
            self.receive(id, connection)
        }
    }

    /// Private (RFC 1918), link-local and loopback IPv4; link-local, unique-local and
    /// loopback IPv6.
    static func isLocal(_ endpoint: NWEndpoint) -> Bool {
        guard case .hostPort(let host, _) = endpoint else { return false }
        switch host {
        case .ipv4(let address):
            let b = [UInt8](address.rawValue)
            guard b.count == 4 else { return false }
            return b[0] == 10 || (b[0] == 172 && (16...31).contains(b[1])) || (b[0] == 192 && b[1] == 168)
                || (b[0] == 169 && b[1] == 254) || b[0] == 127
        case .ipv6(let address):
            let b = [UInt8](address.rawValue)
            guard b.count == 16 else { return false }
            if address.isLoopback || address.isLinkLocal { return true }
            if let v4 = address.asIPv4 { return isLocal(.hostPort(host: .ipv4(v4), port: 0)) }
            return b[0] & 0xFE == 0xFC
        default:
            return false
        }
    }
}
