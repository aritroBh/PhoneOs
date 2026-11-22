import Foundation
import JarvisCore
import Synchronization

/// A `RemoteTransport` without a network: the remote side is an `InMemoryRemoteEndpoint`
/// in the same process. Tests use it, and so does the app's simulator QA mode (UI tests
/// must not touch the local network).
public final class InMemoryRemoteTransport: RemoteTransport {
    private struct State {
        var listening = false
        var events: AsyncStream<RemoteTransportEvent>.Continuation?
        var endpoints: [UInt64: InMemoryRemoteEndpoint] = [:]
        var nextID: UInt64 = 1
        var starts = 0
        /// Bytes delivered as `.received` and not yet reported `consumed`, per connection.
        var unconsumed: [UInt64: Int] = [:]
    }

    private let state = Mutex(State())

    public init() {}

    public var isListening: Bool { state.withLock { $0.listening } }
    /// How many times `start()` was called.
    public var startCount: Int { state.withLock { $0.starts } }
    /// Bytes delivered to the phone side and not yet reported consumed, over all connections.
    public var unconsumedBytes: Int { state.withLock { $0.unconsumed.values.reduce(0, +) } }

    // MARK: Phone side

    public func start() -> AsyncStream<RemoteTransportEvent> {
        let (stream, continuation) = AsyncStream.makeStream(of: RemoteTransportEvent.self)
        let previous = state.withLock { s -> AsyncStream<RemoteTransportEvent>.Continuation? in
            defer { s.events = continuation; s.listening = true; s.starts += 1 }
            return s.events
        }
        previous?.finish()
        continuation.yield(.listening(detail: "in-process (no network)"))
        return stream
    }

    public func send(_ bytes: Data, to connection: UInt64) {
        let endpoint = state.withLock { $0.endpoints[connection] }
        endpoint?.deliver(bytes)
    }

    public func consumed(_ count: Int, on connection: UInt64) {
        state.withLock { s in
            guard let left = s.unconsumed[connection] else { return }
            let rest = left - count
            s.unconsumed[connection] = rest > 0 ? rest : nil
        }
    }

    public func close(_ connection: UInt64) {
        let endpoint = state.withLock { $0.endpoints.removeValue(forKey: connection) }
        endpoint?.closedByPhone()
    }

    public func stop() {
        let (events, endpoints) = state.withLock { s -> (AsyncStream<RemoteTransportEvent>.Continuation?, [InMemoryRemoteEndpoint]) in
            defer { s.events = nil; s.endpoints = [:]; s.listening = false; s.unconsumed = [:] }
            return (s.events, Array(s.endpoints.values))
        }
        endpoints.forEach { $0.closedByPhone() }
        events?.finish()
    }

    /// Test hook: the listener fails the way an `NWListener` can after the app was
    /// suspended. `.failed(why)` is reported and new connections are refused until `start()`
    /// is called again; open connections stay.
    public func failListener(_ why: String = "the listener failed") {
        let events = state.withLock { s -> AsyncStream<RemoteTransportEvent>.Continuation? in
            guard s.listening else { return nil }
            s.listening = false
            return s.events
        }
        events?.yield(.failed(why))
    }

    // MARK: Remote side

    /// Opens a connection to the phone; `nil` while it is not listening.
    public func connect() -> InMemoryRemoteEndpoint? {
        let result = state.withLock { s -> (InMemoryRemoteEndpoint, AsyncStream<RemoteTransportEvent>.Continuation)? in
            guard s.listening, let events = s.events else { return nil }
            let id = s.nextID
            s.nextID += 1
            let endpoint = InMemoryRemoteEndpoint(id: id, transport: self)
            s.endpoints[id] = endpoint
            return (endpoint, events)
        }
        guard let (endpoint, events) = result else { return nil }
        events.yield(.opened(endpoint.id))
        return endpoint
    }

    fileprivate func fromRemote(_ id: UInt64, _ bytes: Data) {
        let events = state.withLock { s -> AsyncStream<RemoteTransportEvent>.Continuation? in
            guard s.endpoints[id] != nil, let events = s.events else { return nil }
            s.unconsumed[id, default: 0] += bytes.count
            return events
        }
        events?.yield(.received(id, bytes))
    }

    fileprivate func remoteClosed(_ id: UInt64) {
        let events = state.withLock { s in s.endpoints.removeValue(forKey: id) != nil ? s.events : nil }
        events?.yield(.closed(id))
    }
}

/// The remote end of one in-memory connection.
public final class InMemoryRemoteEndpoint: Sendable {
    public let id: UInt64
    private let transport: InMemoryRemoteTransport
    private let continuation: AsyncStream<Data>.Continuation
    /// Bytes the phone sent, in order; ends when either side closes.
    public let incoming: AsyncStream<Data>
    private let closed = Mutex(false)

    fileprivate init(id: UInt64, transport: InMemoryRemoteTransport) {
        self.id = id
        self.transport = transport
        (incoming, continuation) = AsyncStream.makeStream(of: Data.self)
    }

    public var isClosed: Bool { closed.withLock { $0 } }

    /// Sends bytes to the phone.
    public func write(_ bytes: Data) {
        guard !isClosed else { return }
        transport.fromRemote(id, bytes)
    }

    /// Closes from the remote side.
    public func close() {
        let wasOpen = closed.withLock { c -> Bool in defer { c = true }; return !c }
        guard wasOpen else { return }
        continuation.finish()
        transport.remoteClosed(id)
    }

    fileprivate func deliver(_ bytes: Data) {
        guard !isClosed else { return }
        continuation.yield(bytes)
    }

    fileprivate func closedByPhone() {
        closed.withLock { $0 = true }
        continuation.finish()
    }
}

/// A paired device in the same process: the real `JarvisRemoteClient` over an
/// `InMemoryRemoteEndpoint`. Tests drive the remote protocol with it, and the app's QA mode
/// uses it to seed a paired device (`-jarvis-seed-remote-device`).
public final class InProcessRemote: Sendable {
    public enum Failure: Error, Equatable, CustomStringConvertible {
        case notListening
        case timedOut(String)
        case closed

        public var description: String {
            switch self {
            case .notListening: "the phone is not listening"
            case .timedOut(let what): "timed out waiting for \(what)"
            case .closed: "the connection closed"
            }
        }
    }

    private struct State {
        var connected = false
        var deviceId: String?
        var credentials: Redacted<String>?
        var messages: [ServerMessage] = []
        var authFailed = false
    }

    private let client: Mutex<JarvisRemoteClient>
    public let endpoint: InMemoryRemoteEndpoint
    private let state = Mutex(State())
    private let pump: Mutex<Task<Void, Never>?> = Mutex(nil)

    private init(client: JarvisRemoteClient, endpoint: InMemoryRemoteEndpoint) {
        self.client = Mutex(client)
        self.endpoint = endpoint
    }

    /// Pairs with the code shown on the phone (typed or scanned) and waits until paired
    /// and connected.
    public static func pair(code: Redacted<String>, name: String, over transport: InMemoryRemoteTransport,
                            random: any RandomSource = GeneratorRandomSource(), timeout: Duration = .seconds(10)) async throws -> InProcessRemote {
        let client = try JarvisRemoteClient.pair(code: code.reveal(), deviceName: name, entropy: try random.bytes(count: 32))
        return try await open(client, over: transport, timeout: timeout)
    }

    /// Reconnects with the credentials from an earlier pairing.
    public static func connect(credentials: Redacted<String>, over transport: InMemoryRemoteTransport,
                               random: any RandomSource = GeneratorRandomSource(), timeout: Duration = .seconds(10)) async throws -> InProcessRemote {
        let client = try JarvisRemoteClient.connect(credentialsJson: credentials.reveal(), entropy: try random.bytes(count: 32))
        return try await open(client, over: transport, timeout: timeout)
    }

    private static func open(_ client: JarvisRemoteClient, over transport: InMemoryRemoteTransport, timeout: Duration) async throws -> InProcessRemote {
        guard let endpoint = transport.connect() else { throw Failure.notListening }
        let remote = InProcessRemote(client: client, endpoint: endpoint)
        remote.startPump()
        try await remote.waitUntil("the connection", timeout: timeout) { $0.connected }
        return remote
    }

    private func startPump() {
        let task = Task { [weak self, endpoint] in
            for await bytes in endpoint.incoming {
                guard let self else { return }
                self.receive(bytes)
            }
        }
        pump.withLock { $0 = task }
    }

    private func receive(_ bytes: Data) {
        let events: [ClientEvent]
        do {
            events = try client.withLock { try $0.receive(bytes: bytes) }
        } catch {
            state.withLock { $0.authFailed = true }
            endpoint.close()
            return
        }
        for event in events {
            switch event {
            case .send(let out):
                endpoint.write(out)
            case .paired(let id, let json):
                state.withLock { $0.deviceId = id; $0.credentials = Redacted(json) }
            case .connected:
                let id = try? client.withLock { try $0.deviceId() }
                state.withLock { s in
                    s.connected = true
                    if s.deviceId == nil { s.deviceId = id ?? nil }
                }
            case .message(let message):
                state.withLock { $0.messages.append(message) }
            case .undecodable:
                break
            }
        }
    }

    public var deviceId: String? { state.withLock { $0.deviceId } }
    /// The credentials from pairing (SECRET: never print `reveal()`).
    public var credentials: Redacted<String>? { state.withLock { $0.credentials } }
    public var messages: [ServerMessage] { state.withLock { $0.messages } }
    public var isClosed: Bool { endpoint.isClosed }

    /// Sends a command to the phone.
    public func send(_ message: ClientMessage) throws {
        let bytes = try client.withLock { try $0.send(message: message) }
        endpoint.write(bytes)
    }

    /// Waits for a message from the phone after the first `after` messages.
    @discardableResult
    public func waitForMessage(after: Int = 0, timeout: Duration = .seconds(10), where match: @escaping @Sendable (ServerMessage) -> Bool) async throws -> ServerMessage {
        var found: ServerMessage?
        try await waitUntil("a matching message", timeout: timeout) { s in
            found = s.messages.dropFirst(after).first(where: match)
            return found != nil
        }
        guard let found else { throw Failure.timedOut("a matching message") }
        return found
    }

    public func close() {
        endpoint.close()
        pump.withLock { $0?.cancel() }
    }

    private func waitUntil(_ what: String, timeout: Duration, _ condition: (State) -> Bool) async throws {
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            let (done, failed) = state.withLock { (condition($0), $0.authFailed) }
            if done { return }
            if failed { throw Failure.closed }
            try await Task.sleep(for: .milliseconds(5))
        }
        throw Failure.timedOut(what)
    }
}
