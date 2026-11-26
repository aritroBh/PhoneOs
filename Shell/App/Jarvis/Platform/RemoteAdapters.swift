import Foundation
import Network
import Security
import Synchronization
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
/// `_jarvis-remote._tcp` (declared in `NSBonjourServices`). Peers are judged by
/// `PeerAddressPolicy` once the connection is ready (local addresses; global IPv6 only on a
/// path that is not cellular); cellular is never used. Bytes are handed to the hub as they
/// arrive (framing is the hub's job), within `RemoteTransportLimits`: a connection with too
/// many received bytes the hub has not consumed, or too many sent bytes the network has not
/// processed, is closed; one that is not ready within 10 s is cancelled. A listener that
/// fails is cancelled and rebuilt after `ListenerRestartPolicy`'s delay while the transport
/// is on.
final class NetworkRemoteTransport: RemoteTransport, @unchecked Sendable {
    static let serviceType = "_jarvis-remote._tcp"

    private struct Entry {
        let connection: NWConnection
        let inbound = ByteBudget(limit: RemoteTransportLimits.maxUnconsumedInboundBytes)
        let outbound = ByteBudget(limit: RemoteTransportLimits.maxUnsentOutboundBytes)
        /// `.opened` was reported (so `.closed` is owed when it goes).
        var opened = false
    }

    private let queue = DispatchQueue(label: "dev.jarvis.remote.transport")
    // Touched on `queue` only.
    private var listener: NWListener?
    private var connections: [UInt64: Entry] = [:]
    private var nextID: UInt64 = 1
    private var continuation: AsyncStream<RemoteTransportEvent>.Continuation?
    private var restart = ListenerRestartPolicy()
    /// Changes with every listener made and with `stop()`: callbacks of an older listener and
    /// rebuilds scheduled for it do nothing.
    private var generation: UInt64 = 0
    /// The listener's port while it is ready (read from any thread for the connection
    /// details).
    private let readyPort = Mutex<UInt16?>(nil)

    func start() -> AsyncStream<RemoteTransportEvent> {
        let (stream, continuation) = AsyncStream.makeStream(of: RemoteTransportEvent.self)
        queue.async {
            self.continuation = continuation
            self.restart = ListenerRestartPolicy()
            self.startListening()
        }
        return stream
    }

    func send(_ bytes: Data, to connection: UInt64) {
        queue.async {
            guard let entry = self.connections[connection] else { return }
            let count = bytes.count
            guard entry.outbound.add(count) else {
                self.drop(connection)
                return
            }
            let outbound = entry.outbound
            entry.connection.send(content: bytes, completion: .contentProcessed { _ in outbound.remove(count) })
        }
    }

    /// As `send(_:to:)`; `written` runs from the same `.contentProcessed` completion (the
    /// hub's Live view backpressure). A connection dropped first never confirms.
    func send(_ bytes: Data, to connection: UInt64, written: @escaping @Sendable (Int) -> Void) {
        queue.async {
            guard let entry = self.connections[connection] else { return }
            let count = bytes.count
            guard entry.outbound.add(count) else {
                self.drop(connection)
                return
            }
            let outbound = entry.outbound
            entry.connection.send(content: bytes, completion: .contentProcessed { (error: NWError?) in
                outbound.remove(count)
                if error == nil { written(count) }
            })
        }
    }

    func consumed(_ count: Int, on connection: UInt64) {
        queue.async {
            self.connections[connection]?.inbound.remove(count)
        }
    }

    func close(_ connection: UInt64) {
        queue.async {
            self.connections.removeValue(forKey: connection)?.connection.cancel()
        }
    }

    /// This phone's local IPv4 addresses on Wi-Fi (or Ethernet) with the listener's port,
    /// for the Remote control screen's connection details. Only addresses `PeerAddressPolicy`
    /// accepts (local ones) are listed; empty while the listener is not ready.
    func localAddresses() -> [RemotePhoneAddress] {
        guard let port = readyPort.withLock({ $0 }) else { return [] }
        return Self.localIPv4Addresses().map { .host($0, port: port) }
    }

    static func localIPv4Addresses() -> [String] {
        var list: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&list) == 0, let first = list else { return [] }
        defer { freeifaddrs(list) }
        var found: [String] = []
        var cursor: UnsafeMutablePointer<ifaddrs>? = first
        while let entry = cursor {
            defer { cursor = entry.pointee.ifa_next }
            let flags = Int32(bitPattern: entry.pointee.ifa_flags)
            guard let address = entry.pointee.ifa_addr, address.pointee.sa_family == UInt8(AF_INET),
                  flags & IFF_UP != 0, flags & IFF_LOOPBACK == 0 else { continue }
            let name = String(cString: entry.pointee.ifa_name)
            guard name.hasPrefix("en") else { continue }
            var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            let status = getnameinfo(address, socklen_t(address.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST)
            guard status == 0 else { continue }
            let text = host.withUnsafeBufferPointer { buffer in buffer.baseAddress.map { String(cString: $0) } ?? "" }
            guard let bytes = RemotePhoneAddress.ipv4Bytes(text),
                  PeerAddressPolicy.decide(address: bytes, pathIsCellular: false) == .accept,
                  !found.contains(text) else { continue }
            found.append(text)
        }
        return found
    }

    func stop() {
        readyPort.withLock { $0 = nil }
        queue.async {
            self.generation &+= 1
            self.listener?.cancel()
            self.listener = nil
            for entry in self.connections.values { entry.connection.cancel() }
            self.connections = [:]
            self.continuation?.finish()
            self.continuation = nil
        }
    }

    // MARK: - On the queue

    private static func nowMs() -> UInt64 { DispatchTime.now().uptimeNanoseconds / 1_000_000 }

    private func startListening() {
        guard let continuation else { return }
        generation &+= 1
        let generation = self.generation
        let parameters = NWParameters.tcp
        parameters.includePeerToPeer = false
        parameters.prohibitedInterfaceTypes = [.cellular]
        do {
            let listener = try NWListener(using: parameters)
            listener.service = NWListener.Service(type: Self.serviceType)
            listener.stateUpdateHandler = { [weak self] state in self?.listenerChanged(state, generation: generation) }
            listener.newConnectionHandler = { [weak self] connection in self?.accept(connection) }
            listener.start(queue: queue)
            self.listener = listener
        } catch {
            continuation.yield(.failed("the listener could not start (\(error))"))
            scheduleRebuild()
        }
    }

    private func listenerChanged(_ state: NWListener.State, generation: UInt64) {
        guard generation == self.generation else { return }
        switch state {
        case .ready:
            restart.listenerReady(nowMs: Self.nowMs())
            let port = listener?.port.map { " on port \($0.rawValue)" } ?? ""
            let raw = listener?.port?.rawValue
            readyPort.withLock { $0 = raw }
            continuation?.yield(.listening(detail: "Same Wi-Fi, \(Self.serviceType)\(port)"))
        case .failed(let error):
            readyPort.withLock { $0 = nil }
            continuation?.yield(.failed("the listener stopped (\(error))"))
            listener?.cancel()
            listener = nil
            scheduleRebuild()
        case .waiting(let error):
            continuation?.yield(.failed("waiting for the network (\(error))"))
        default:
            break
        }
    }

    /// Rebuilds the listener after the policy's delay, unless the transport was stopped or
    /// restarted meanwhile.
    private func scheduleRebuild() {
        let delay = restart.listenerFailed(nowMs: Self.nowMs())
        let generation = self.generation
        queue.asyncAfter(deadline: .now() + .milliseconds(Int(clamping: delay))) { [weak self] in
            guard let self, self.generation == generation, self.continuation != nil, self.listener == nil else { return }
            self.startListening()
        }
    }

    private func accept(_ connection: NWConnection) {
        guard connections.count < RemoteTransportLimits.maxConnections else {
            connection.cancel()
            return
        }
        let id = nextID
        nextID += 1
        connections[id] = Entry(connection: connection)
        connection.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready:
                self.ready(id)
            case .failed, .cancelled:
                self.drop(id)
            default:
                break
            }
        }
        connection.start(queue: queue)
        // A connection that never becomes ready must not stay forever.
        queue.asyncAfter(deadline: .now() + .milliseconds(RemoteTransportLimits.connectionReadyTimeoutMs)) { [weak self] in
            guard let self, let entry = self.connections[id], !entry.opened else { return }
            self.drop(id)
        }
    }

    /// The peer is judged now that the path is known; refused peers are cancelled before
    /// the hub hears of them.
    private func ready(_ id: UInt64) {
        guard let connection = connections[id]?.connection else { return }
        let cellular = connection.currentPath.map { $0.usesInterfaceType(.cellular) }
        guard PeerAddressPolicy.decide(address: Self.addressBytes(connection.endpoint), pathIsCellular: cellular) == .accept else {
            drop(id)
            return
        }
        connections[id]?.opened = true
        continuation?.yield(.opened(id))
        receive(id, connection)
    }

    private func receive(_ id: UInt64, _ connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1_024) { [weak self] data, _, isComplete, error in
            guard let self, let entry = self.connections[id] else { return }
            if let data, !data.isEmpty {
                guard entry.inbound.add(data.count) else {
                    // The peer sends faster than the hub consumes: close it.
                    self.drop(id)
                    return
                }
                self.continuation?.yield(.received(id, data))
            }
            if isComplete || error != nil {
                self.drop(id)
                return
            }
            self.receive(id, connection)
        }
    }

    /// Cancels and forgets connection `id`; the hub hears `.closed` if it heard `.opened`.
    private func drop(_ id: UInt64) {
        guard let entry = connections.removeValue(forKey: id) else { return }
        entry.connection.cancel()
        if entry.opened { continuation?.yield(.closed(id)) }
    }

    /// The peer's address bytes (4 or 16), empty for anything but an IP host and port.
    static func addressBytes(_ endpoint: NWEndpoint) -> [UInt8] {
        guard case .hostPort(let host, _) = endpoint else { return [] }
        switch host {
        case .ipv4(let address): return [UInt8](address.rawValue)
        case .ipv6(let address): return [UInt8](address.rawValue)
        default: return []
        }
    }
}
