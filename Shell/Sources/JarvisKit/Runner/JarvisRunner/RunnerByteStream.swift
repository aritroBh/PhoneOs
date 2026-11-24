import Foundation
import Synchronization

/// One connection's bytes, as Jarvis's own runner protocol sees them: an ordered,
/// reliable stream (a loopback TCP connection on the phone, an in-memory pipe in tests).
/// Framing, authentication and everything else is the core's (`JarvisRunnerClientChannel`,
/// `JarvisRunnerServerChannel`); a stream only moves bytes.
public protocol RunnerByteStream: Sendable {
    /// Writes `bytes` after everything written before. Throws when the connection is gone.
    func write(_ bytes: Data) async throws
    /// The next bytes that arrived (never empty), or `nil` at the end of the stream (the
    /// peer closed, or `close()` was called). Throws when the connection failed.
    func read() async throws -> Data?
    /// Closes the connection. Idempotent; a pending `read` returns `nil` or throws.
    func close() async
}

/// Opens connections to the runner (the app's side).
public protocol RunnerConnector: Sendable {
    /// A new connection, or a throw when nothing answers.
    func connect() async throws -> any RunnerByteStream
}

/// The connection was closed or broke.
public struct RunnerStreamClosed: Error, Equatable, CustomStringConvertible {
    public init() {}
    public var description: String { "the runner connection is closed" }
}

/// Two connected in-memory ends: bytes written to one are read from the other. For tests
/// (client and server in one process, Linux included) and the QA harness.
public enum InMemoryRunnerPipe {
    /// A connected pair: (`a`, `b`).
    public static func make() -> (InMemoryRunnerStream, InMemoryRunnerStream) {
        let toA = InMemoryRunnerStream.Queue()
        let toB = InMemoryRunnerStream.Queue()
        return (
            InMemoryRunnerStream(incoming: toA, outgoing: toB),
            InMemoryRunnerStream(incoming: toB, outgoing: toA)
        )
    }
}

/// One end of an `InMemoryRunnerPipe`. Closing either end closes both directions.
public final class InMemoryRunnerStream: RunnerByteStream {
    /// One direction's bytes, with at most one waiting reader.
    final class Queue: Sendable {
        private struct State {
            var chunks: [Data] = []
            var closed = false
            var waiter: CheckedContinuation<Data?, Never>?
        }

        private let state = Mutex(State())

        /// Appends `bytes`; `false` when the queue is closed.
        func push(_ bytes: Data) -> Bool {
            let (accepted, waiter) = state.withLock { s -> (Bool, CheckedContinuation<Data?, Never>?) in
                if s.closed { return (false, nil) }
                if let waiter = s.waiter {
                    s.waiter = nil
                    return (true, waiter)
                }
                s.chunks.append(bytes)
                return (true, nil)
            }
            waiter?.resume(returning: bytes)
            return accepted
        }

        /// The next chunk, or `nil` once the queue is closed (closing drops what is unread).
        func pop() async -> Data? {
            await withCheckedContinuation { (continuation: CheckedContinuation<Data?, Never>) in
                let ready: Data?? = state.withLock { s in
                    if !s.chunks.isEmpty { return .some(s.chunks.removeFirst()) }
                    if s.closed { return .some(nil) }
                    if let old = s.waiter {
                        // A second concurrent reader: the older one gets the end.
                        s.waiter = nil
                        old.resume(returning: nil)
                    }
                    s.waiter = continuation
                    return .none
                }
                if case .some(let value) = ready { continuation.resume(returning: value) }
            }
        }

        func close() {
            let waiter = state.withLock { s -> CheckedContinuation<Data?, Never>? in
                s.closed = true
                s.chunks = []
                defer { s.waiter = nil }
                return s.waiter
            }
            waiter?.resume(returning: nil)
        }

        var isClosed: Bool { state.withLock { $0.closed } }
    }

    private let incoming: Queue
    private let outgoing: Queue
    /// Every byte written through this end, in order (tests record and replay sessions).
    public let written = Mutex(Data())

    init(incoming: Queue, outgoing: Queue) {
        self.incoming = incoming
        self.outgoing = outgoing
    }

    /// Whether this end can no longer write.
    public var isClosed: Bool { outgoing.isClosed }

    public func write(_ bytes: Data) async throws {
        guard !bytes.isEmpty else {
            if outgoing.isClosed { throw RunnerStreamClosed() }
            return
        }
        written.withLock { $0.append(bytes) }
        guard outgoing.push(bytes) else { throw RunnerStreamClosed() }
    }

    public func read() async throws -> Data? {
        await incoming.pop()
    }

    public func close() async {
        incoming.close()
        outgoing.close()
    }
}
