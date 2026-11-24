import Foundation
import JarvisCore
import Synchronization

/// `PhoneRunner` over Jarvis's own runner protocol: the typed tree the safety gate trusts,
/// from Jarvis's own XCUITest runner process, over an authenticated loopback connection
/// (`docs/JARVIS-RUNNER.md`).
///
/// - **Authentication first.** A connection is used only after the runner proved the
///   shared key (`JarvisRunnerClientChannel`); until then nothing is sent but the hello.
///   A program on the port that cannot prove it (another app holding the port) is treated
///   as no runner at all: `JarvisRunnerError.unauthenticated`, never a screen.
/// - **One request at a time**, in call order; each with a deadline. A timeout, a broken
///   connection or a protocol error drops the connection; the next call connects again.
/// - **Errors keep their meaning.** A command the runner refused (locked, stale, could
///   not) is a `RunnerResult` with its `failure`, as from every runner; a throw is "could
///   not reach or understand the runner". The runner's lock state and front app pass
///   through unchanged.
public final class JarvisRunnerClient: PhoneRunner {
    public struct Configuration: Sendable {
        /// Connect, prove the key both ways and exchange hellos.
        public var connectTimeout: Duration
        /// A snapshot, lock read, screenshot or front-app read (longer than the server's
        /// own deadline, so its error reply arrives first).
        public var readTimeout: Duration
        /// A command (plus its own duration for `wait`).
        public var commandTimeout: Duration

        public init(
            connectTimeout: Duration = .seconds(5),
            readTimeout: Duration = .seconds(35),
            commandTimeout: Duration = .seconds(50)
        ) {
            self.connectTimeout = connectTimeout
            self.readTimeout = readTimeout
            self.commandTimeout = commandTimeout
        }
    }

    /// One authenticated connection.
    final class Connection: Sendable {
        let stream: any RunnerByteStream
        let channel: JarvisRunnerClientChannel

        init(stream: any RunnerByteStream, channel: JarvisRunnerClientChannel) {
            self.stream = stream
            self.channel = channel
        }

        /// Sends `request` and reads until its reply.
        func exchange(_ request: RunnerWireRequest) async throws -> RunnerWireReply {
            let out = try channel.request(request: request)
            try await stream.write(out.bytes)
            while let bytes = try await stream.read() {
                for event in try channel.receive(bytes: bytes) {
                    switch event {
                    case .send(let more):
                        try await stream.write(more)
                    case .authenticated:
                        throw JarvisRunnerError.protocolError("a second handshake")
                    case .reply(let id, let reply):
                        guard id == out.requestId else { throw JarvisRunnerError.protocolError("a reply to another request") }
                        return reply
                    }
                }
            }
            throw JarvisRunnerError.unreachable("the runner closed the connection")
        }
    }

    /// Holds a stream being set up, so a timeout can close it.
    private final class PendingStream: Sendable {
        let stream = Mutex<(any RunnerByteStream)?>(nil)
    }

    private let connector: any RunnerConnector
    private let keySource: any RunnerKeySource
    private let random: any RandomSource
    private let configuration: Configuration
    private let gate = RunnerAsyncGate()
    private let current = Mutex<Connection?>(nil)

    public init(
        connector: any RunnerConnector,
        keySource: any RunnerKeySource,
        random: any RandomSource,
        configuration: Configuration = Configuration()
    ) {
        self.connector = connector
        self.keySource = keySource
        self.random = random
        self.configuration = configuration
    }

    // MARK: - PhoneRunner

    public func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        switch try await call(.snapshot(includeScreenshot: includeScreenshot), configuration.readTimeout) {
        case .snapshot(let snapshot): return snapshot.observation
        case .error(let message): throw JarvisRunnerError.runnerFailed(message)
        default: throw JarvisRunnerError.protocolError("not a snapshot")
        }
    }

    public func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        var limit = configuration.commandTimeout
        if case .wait(let ms) = command { limit += .milliseconds(Int64(clamping: ms)) }
        // `expect` stays with the driver's FrontAppGuard, which checked it before this call.
        switch try await call(.perform(command: command, expect: nil), limit) {
        case .performed(let result): return result
        case .error(let message): throw JarvisRunnerError.runnerFailed(message)
        default: throw JarvisRunnerError.protocolError("not a command result")
        }
    }

    public func lockState() async throws -> DeviceLockState {
        switch try await call(.lockState, configuration.readTimeout) {
        case .lockState(let state): return state.deviceLockState
        case .error(let message): throw JarvisRunnerError.runnerFailed(message)
        default: throw JarvisRunnerError.protocolError("not a lock state")
        }
    }

    public func viewerScreenshot() async throws -> CapturedScreenshot? {
        switch try await call(.viewerScreenshot, configuration.readTimeout) {
        case .viewerScreenshot(let shot): return shot?.captured
        case .error(let message): throw JarvisRunnerError.runnerFailed(message)
        default: throw JarvisRunnerError.protocolError("not a screenshot")
        }
    }

    public func frontBundleId() async throws -> String? {
        switch try await call(.frontBundleId, configuration.readTimeout) {
        case .frontBundleId(let id): return id
        case .error(let message): throw JarvisRunnerError.runnerFailed(message)
        default: throw JarvisRunnerError.protocolError("not a front app")
        }
    }

    /// Whether a connection is up now (for status displays; no I/O).
    public var isConnected: Bool { current.withLock { $0 != nil } }

    /// Drops the connection; the next call connects again.
    public func disconnect() async {
        let old = current.withLock { c -> Connection? in
            defer { c = nil }
            return c
        }
        await old?.stream.close()
    }

    // MARK: - Calls

    private func call(_ request: RunnerWireRequest, _ limit: Duration) async throws -> RunnerWireReply {
        await gate.acquire()
        do {
            let reply = try await callHoldingGate(request, limit)
            await gate.release()
            return reply
        } catch {
            await gate.release()
            throw error
        }
    }

    private func callHoldingGate(_ request: RunnerWireRequest, _ limit: Duration) async throws -> RunnerWireReply {
        let connection = try await connection()
        do {
            return try await IORace<RunnerWireReply>().run(kind: .act, timeout: limit) {
                try await connection.exchange(request)
            }
        } catch {
            await drop(connection)
            if error is CancellationError { throw error }
            if case DriverIOError.timedOut = error {
                throw JarvisRunnerError.timedOut(operation: request.operationName, after: limit)
            }
            throw Self.mapped(error, authenticated: true)
        }
    }

    private func drop(_ connection: Connection) async {
        current.withLock { c in
            if c === connection { c = nil }
        }
        await connection.stream.close()
    }

    /// The current connection, or a new authenticated one.
    private func connection() async throws -> Connection {
        if let existing = current.withLock({ $0 }) { return existing }
        guard let key = try keySource.runnerKey() else { throw JarvisRunnerError.notProvisioned }
        let pending = PendingStream()
        let connector = connector
        let random = random
        let made: Connection
        do {
            made = try await IORace<Connection>().run(kind: .observe, timeout: configuration.connectTimeout) {
                try await Self.open(connector: connector, key: key, random: random, pending: pending)
            }
        } catch {
            let stream = pending.stream.withLock { $0 }
            await stream?.close()
            if error is CancellationError { throw error }
            if case DriverIOError.timedOut = error {
                throw JarvisRunnerError.timedOut(operation: "connect", after: configuration.connectTimeout)
            }
            throw Self.mapped(error, authenticated: false)
        }
        current.withLock { $0 = made }
        return made
    }

    /// Connects, proves the key both ways and exchanges hellos.
    private static func open(
        connector: any RunnerConnector,
        key: Redacted<Data>,
        random: any RandomSource,
        pending: PendingStream
    ) async throws -> Connection {
        let stream: any RunnerByteStream
        do {
            stream = try await connector.connect()
        } catch {
            throw JarvisRunnerError.unreachable(String(describing: error))
        }
        pending.stream.withLock { $0 = stream }
        let channel = try JarvisRunnerClientChannel(key: key.reveal(), entropy: random.bytes(count: 32))
        try await stream.write(try channel.start())
        var authenticated = false
        while !authenticated {
            guard let bytes = try await stream.read() else {
                throw JarvisRunnerError.unreachable("the connection closed before the runner proved the key")
            }
            for event in try channel.receive(bytes: bytes) {
                switch event {
                case .send(let out): try await stream.write(out)
                case .authenticated: authenticated = true
                case .reply: throw JarvisRunnerError.protocolError("a reply before any request")
                }
            }
        }
        let connection = Connection(stream: stream, channel: channel)
        let hello: RunnerWireReply
        do {
            hello = try await connection.exchange(.hello(protocolVersion: runnerProtocolVersion()))
        } catch {
            throw mapped(error, authenticated: true)
        }
        guard case .hello(let version, _) = hello, version == runnerProtocolVersion() else {
            throw JarvisRunnerError.protocolError("the runner speaks another protocol version")
        }
        return connection
    }

    /// The client's error for anything thrown while talking to the runner. Before the
    /// runner proved the key, every channel failure means "not our runner".
    static func mapped(_ error: any Error, authenticated: Bool) -> JarvisRunnerError {
        if let error = error as? JarvisRunnerError { return error }
        if case .RunnerChannel(let problem, let message) = error as? JarvisError {
            switch problem {
            case .key: return .notProvisioned
            case .entropy: return .unreachable(message)
            default: break
            }
            if !authenticated { return .unauthenticated }
            switch problem {
            case .authentication: return .unauthenticated
            case .closed: return .unreachable(message)
            default: return .protocolError(message)
            }
        }
        if error is RunnerStreamClosed { return .unreachable(String(describing: error)) }
        return .unreachable(CoreErrorMessage(error).message)
    }
}
