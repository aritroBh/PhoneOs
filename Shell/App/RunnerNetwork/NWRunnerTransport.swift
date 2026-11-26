import Foundation
import Network
import Security
import JarvisKit

// Loopback TCP for Jarvis's own runner protocol (docs/JARVIS-RUNNER.md). Compiled into the
// app (the connector), the JarvisRunner UI-test bundle (the listener) and JarvisUITests
// (both, for the simulator proof). Only bytes move here: framing and authentication are
// the core's `JarvisRunnerClientChannel` / `JarvisRunnerServerChannel`, and nothing read
// from a connection is trusted before the peer proved the key.

/// Resumes a continuation once, whichever callback comes first.
private final class ResumeOnce<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Value, any Error>?

    init(_ continuation: CheckedContinuation<Value, any Error>) {
        self.continuation = continuation
    }

    func resume(_ result: Result<Value, any Error>) {
        lock.lock()
        let continuation = self.continuation
        self.continuation = nil
        lock.unlock()
        continuation?.resume(with: result)
    }
}

/// `SecRandomCopyBytes` for the runner channels' per-connection entropy.
struct SystemRunnerRandom: RandomSource {
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

/// A `RunnerByteStream` over one `NWConnection`.
final class NWRunnerStream: RunnerByteStream, @unchecked Sendable {
    private let connection: NWConnection
    /// Review pass 32 (F3): set once this stream knows the connection is over (guarded by
    /// `lock`; the connection's callbacks arrive on its own queue).
    private let lock = NSLock()
    private var over = false

    init(_ connection: NWConnection) {
        self.connection = connection
    }

    /// Review pass 32 (F3): `true` once the stream knows the connection is over — its own
    /// `close()` was called, a receive saw the peer's end or an error, a send failed, or the
    /// connection's state is `.cancelled` or `.failed` — so `RunnerServer.accept` frees the
    /// slot for a quick reconnect. Never `true` for a live connection (a second local
    /// connection must not evict an authenticated one).
    var isKnownClosed: Bool {
        lock.lock()
        let known = over
        lock.unlock()
        if known { return true }
        switch connection.state {
        case .cancelled, .failed:
            return true
        default:
            return false
        }
    }

    private func markOver() {
        lock.lock()
        over = true
        lock.unlock()
    }

    func write(_ bytes: Data) async throws {
        let connection = connection
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            connection.send(content: bytes, completion: .contentProcessed { error in
                if let error {
                    self.markOver()
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            })
        }
    }

    func read() async throws -> Data? {
        let connection = connection
        while true {
            let chunk: Data? = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Data?, any Error>) in
                connection.receive(minimumIncompleteLength: 1, maximumLength: 256 * 1_024) { data, _, isComplete, error in
                    if let data, !data.isEmpty {
                        continuation.resume(returning: data)
                    } else if let error {
                        self.markOver()
                        continuation.resume(throwing: error)
                    } else if isComplete {
                        self.markOver()
                        continuation.resume(returning: nil)
                    } else {
                        continuation.resume(returning: Data())
                    }
                }
            }
            guard let chunk else { return nil }
            if !chunk.isEmpty { return chunk }
        }
    }

    func close() async {
        markOver()
        connection.cancel()
    }
}

/// Connects to the runner on 127.0.0.1 (the app's side).
struct NWRunnerConnector: RunnerConnector {
    struct NotReady: Error, CustomStringConvertible {
        var detail: String
        var description: String { "no runner on 127.0.0.1 (\(detail))" }
    }

    var port: UInt16 = JarvisRunnerDefaults.port

    func connect() async throws -> any RunnerByteStream {
        guard let nwPort = NWEndpoint.Port(rawValue: port), port != 0 else { throw NotReady(detail: "port \(port)") }
        let connection = NWConnection(host: .ipv4(.loopback), port: nwPort, using: .tcp)
        let queue = DispatchQueue(label: "dev.jarvis.runner.client")
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            let once = ResumeOnce(continuation)
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    once.resume(.success(()))
                case .failed(let error):
                    once.resume(.failure(NotReady(detail: "\(error)")))
                case .waiting(let error):
                    // Loopback with nothing listening: refused. Do not wait for a path.
                    once.resume(.failure(NotReady(detail: "\(error)")))
                    connection.cancel()
                case .cancelled:
                    once.resume(.failure(NotReady(detail: "cancelled")))
                default:
                    break
                }
            }
            connection.start(queue: queue)
        }
        return NWRunnerStream(connection)
    }
}

/// Listens on 127.0.0.1 only and hands every connection to a `RunnerServer` (which serves
/// one at a time and closes the rest).
final class NWRunnerListener: @unchecked Sendable {
    struct Failed: Error, CustomStringConvertible {
        var detail: String
        var description: String { "the runner listener could not start (\(detail))" }
    }

    private let server: RunnerServer
    private let queue = DispatchQueue(label: "dev.jarvis.runner.listener")
    private var listener: NWListener?

    init(server: RunnerServer) {
        self.server = server
    }

    /// Starts listening on 127.0.0.1:`port` (0: any free port) and returns the bound port.
    /// A port another app holds fails here (spike R4).
    func start(port: UInt16) async throws -> UInt16 {
        let parameters = NWParameters.tcp
        parameters.includePeerToPeer = false
        let localPort = NWEndpoint.Port(rawValue: port) ?? .any
        parameters.requiredLocalEndpoint = .hostPort(host: .ipv4(.loopback), port: localPort)
        let listener: NWListener
        do {
            listener = try NWListener(using: parameters)
        } catch {
            throw Failed(detail: "\(error)")
        }
        let server = server
        let queue = queue
        listener.newConnectionHandler = { connection in
            // Bound to loopback, so only local peers arrive; checked again all the same.
            guard NWRunnerListener.isLoopback(connection.endpoint) else {
                connection.cancel()
                return
            }
            connection.start(queue: queue)
            let stream = NWRunnerStream(connection)
            Task { await server.accept(stream) }
        }
        self.listener = listener
        return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<UInt16, any Error>) in
            let once = ResumeOnce(continuation)
            listener.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    once.resume(.success(listener.port?.rawValue ?? port))
                case .failed(let error):
                    once.resume(.failure(Failed(detail: "\(error)")))
                    listener.cancel()
                case .waiting(let error):
                    once.resume(.failure(Failed(detail: "\(error)")))
                    listener.cancel()
                case .cancelled:
                    once.resume(.failure(Failed(detail: "cancelled")))
                default:
                    break
                }
            }
            listener.start(queue: queue)
        }
    }

    func stop() {
        listener?.cancel()
        listener = nil
    }

    static func isLoopback(_ endpoint: NWEndpoint) -> Bool {
        guard case .hostPort(let host, _) = endpoint else { return false }
        switch host {
        case .ipv4(let address): return address == .loopback || address.rawValue.first == 127
        case .ipv6(let address): return address == .loopback
        case .name(let name, _): return name == "localhost"
        @unknown default: return false
        }
    }
}
