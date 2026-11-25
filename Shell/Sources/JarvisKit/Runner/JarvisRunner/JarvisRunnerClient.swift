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
/// - **One request at a time**, in call order; each with a deadline (`JarvisRunnerTiming`:
///   inside the driver's, outside the server's) that covers connecting too and travels
///   with the request as its `not_after_ms`, so the runner never acts on it later. A
///   timeout, a broken connection or a protocol error drops the connection; the next call
///   connects again. A connection idle for `idleRetire` is replaced before use (the server
///   closes idle ones). A call whose connection turns out closed is made once more on a
///   fresh connection when that is safe (`mayRetry`): never a `perform` that may have left.
/// - **The stale-screen check happens at the runner.** `perform(_:expect:)` sends the
///   command's `expect`; the runner compares it with the app in front next to the gesture
///   (`verifiesFrontApp`).
/// - **Errors keep their meaning.** A command the runner refused (locked, stale, could
///   not) is a `RunnerResult` with its `failure`, as from every runner; a throw is "could
///   not reach or understand the runner". The runner's lock state and front app pass
///   through unchanged.
public final class JarvisRunnerClient: PhoneRunner {
    public struct Configuration: Sendable {
        /// Connect, prove the key both ways and exchange hellos (also bounded by the
        /// call's own deadline).
        public var connectTimeout: Duration
        /// A snapshot, lock read, screenshot or front-app read, connecting included: longer
        /// than the server's own deadline (so its error reply arrives first) and shorter
        /// than the driver's (`JarvisRunnerTiming`).
        public var readTimeout: Duration
        /// A command (plus its own duration for `wait`), connecting included; same order.
        public var commandTimeout: Duration
        /// A connection unused this long is replaced before the next call.
        public var idleRetire: Duration

        public init(
            connectTimeout: Duration = .seconds(5),
            readTimeout: Duration = JarvisRunnerTiming.clientRead,
            commandTimeout: Duration = JarvisRunnerTiming.clientCommand,
            idleRetire: Duration = JarvisRunnerTiming.clientIdleRetire
        ) {
            self.connectTimeout = connectTimeout
            self.readTimeout = readTimeout
            self.commandTimeout = commandTimeout
            self.idleRetire = idleRetire
        }
    }

    /// One authenticated connection.
    final class Connection: Sendable {
        let stream: any RunnerByteStream
        let channel: JarvisRunnerClientChannel
        /// When the last exchange ended (the shared uptime clock, ms).
        let lastUsedMs: Mutex<UInt64>

        init(stream: any RunnerByteStream, channel: JarvisRunnerClientChannel, nowMs: UInt64) {
            self.stream = stream
            self.channel = channel
            self.lastUsedMs = Mutex(nowMs)
        }

        /// Sends `request`, which the runner may act on until `notAfterMs`, and reads until
        /// its reply.
        ///
        /// Throws `RequestNotSent` when the connection was already closed before any byte of
        /// the request left this process (the stream says it is closed, or its write refused
        /// with `RunnerStreamClosed`, which promises nothing was sent).
        func exchange(_ request: RunnerWireRequest, notAfterMs: UInt64) async throws -> RunnerWireReply {
            if stream.isKnownClosed { throw RequestNotSent(underlying: RunnerStreamClosed()) }
            let out = try channel.request(request: request, notAfterMs: notAfterMs)
            do {
                try await stream.write(out.bytes)
            } catch let error as RunnerStreamClosed {
                throw RequestNotSent(underlying: error)
            }
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

    /// Holds a stream being set up, so a timeout can close it, and so a connect that
    /// completes after the caller gave up closes its own stream instead of holding the
    /// runner's only slot.
    final class PendingStream: Sendable {
        private struct State {
            var stream: (any RunnerByteStream)?
            var abandoned = false
        }

        private let state = Mutex(State())

        /// Keeps `stream`; `false` when the caller already gave up (the caller must close
        /// it and stop).
        func keep(_ stream: any RunnerByteStream) -> Bool {
            state.withLock { s in
                if s.abandoned { return false }
                s.stream = stream
                return true
            }
        }

        /// The caller gives up: returns the stream to close, if one was kept; any stream
        /// that arrives later is refused by `keep`.
        func abandon() -> (any RunnerByteStream)? {
            state.withLock { s in
                s.abandoned = true
                defer { s.stream = nil }
                return s.stream
            }
        }
    }

    private let connector: any RunnerConnector
    private let keySource: any RunnerKeySource
    private let random: any RandomSource
    private let configuration: Configuration
    private let clock: @Sendable () -> UInt64
    private let gate = RunnerAsyncGate()
    private let current = Mutex<Connection?>(nil)

    /// `clock`: milliseconds of the uptime clock the runner process reads too
    /// (`RunnerUptimeClock.nowMs`; tests pass their own).
    public init(
        connector: any RunnerConnector,
        keySource: any RunnerKeySource,
        random: any RandomSource,
        configuration: Configuration = Configuration(),
        clock: @escaping @Sendable () -> UInt64 = RunnerUptimeClock.nowMs
    ) {
        self.connector = connector
        self.keySource = keySource
        self.random = random
        self.configuration = configuration
        self.clock = clock
    }

    /// The runner checks `expect` next to the gesture, and a front-app read that throws
    /// means the channel failed (`FrontAppGuard` then refuses the command).
    public var verifiesFrontApp: Bool { true }

    // MARK: - PhoneRunner

    public func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        switch try await call(.snapshot(includeScreenshot: includeScreenshot), configuration.readTimeout) {
        case .snapshot(let snapshot): return snapshot.observation
        case .error(let message): throw JarvisRunnerError.runnerFailed(message)
        default: throw JarvisRunnerError.protocolError("not a snapshot")
        }
    }

    public func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        try await perform(command, expect: nil)
    }

    /// Sends `command` with `expect`: the runner refuses it as `stale` (nothing done) when
    /// another app is in front, or none can be read, at the moment of the gesture.
    public func perform(_ command: RunnerCommand, expect: CommandExpect?) async throws -> RunnerResult {
        var limit = configuration.commandTimeout
        if case .wait(let ms) = command { limit += .milliseconds(Int64(clamping: ms)) }
        switch try await call(.perform(command: command, expect: expect), limit) {
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
        // One deadline for the whole call, connecting and one retry included; it is also the
        // request's `not_after_ms`, so the runner never acts on it after this call gave up.
        let startedMs = clock()
        let notAfterMs = startedMs &+ limit.wholeMilliseconds
        var retried = false
        while true {
            let connection = try await connection(notAfterMs: notAfterMs)
            let remaining = limit - .milliseconds(Int64(clamping: clock() &- startedMs))
            guard remaining > .zero else {
                await drop(connection)
                throw JarvisRunnerError.timedOut(operation: request.operationName, after: limit)
            }
            let clock = clock
            do {
                let reply = try await IORace<RunnerWireReply>().run(kind: .act, timeout: remaining) {
                    try await connection.exchange(request, notAfterMs: notAfterMs)
                }
                connection.lastUsedMs.withLock { $0 = clock() }
                return reply
            } catch {
                await drop(connection)
                if error is CancellationError { throw error }
                if case DriverIOError.timedOut = error {
                    throw JarvisRunnerError.timedOut(operation: request.operationName, after: limit)
                }
                // Once, on a fresh connection (new channel, new entropy).
                if !retried, Self.mayRetry(after: error, request) {
                    retried = true
                    continue
                }
                throw Self.mapped(error, authenticated: true)
            }
        }
    }

    /// Whether a call that failed with `error` may be made again on a fresh connection:
    /// - nothing of the request was sent (the peer had closed the connection, e.g. the
    ///   runner's idle cut): any request, `perform` included;
    /// - a read (snapshot, lock state, screenshot, front app) whose connection failed after
    ///   it was written: reads have no effect, so asking again is safe.
    /// A `perform` any byte of which may have left is never sent again (no duplicate
    /// commands); its failure is reported.
    static func mayRetry(after error: any Error, _ request: RunnerWireRequest) -> Bool {
        if error is RequestNotSent { return true }
        switch request {
        case .snapshot, .lockState, .viewerScreenshot, .frontBundleId:
            if case .unreachable = mapped(error, authenticated: true) { return true }
            return false
        case .hello, .perform:
            return false
        }
    }

    private func drop(_ connection: Connection) async {
        current.withLock { c in
            if c === connection { c = nil }
        }
        await connection.stream.close()
    }

    /// The current connection (unless idle too long), or a new authenticated one whose
    /// hello the runner may act on until `notAfterMs` at the latest.
    private func connection(notAfterMs: UInt64) async throws -> Connection {
        if let existing = current.withLock({ $0 }) {
            let idleMs = clock() &- existing.lastUsedMs.withLock { $0 }
            if Duration.milliseconds(Int64(clamping: idleMs)) < configuration.idleRetire { return existing }
            // The server closes idle connections: start afresh rather than meet a closed one.
            await drop(existing)
        }
        guard let key = try keySource.runnerKey() else { throw JarvisRunnerError.notProvisioned }
        let pending = PendingStream()
        let connector = connector
        let random = random
        let clock = clock
        let helloNotAfterMs = min(notAfterMs, clock() &+ configuration.connectTimeout.wholeMilliseconds)
        let made: Connection
        do {
            made = try await IORace<Connection>().run(kind: .observe, timeout: configuration.connectTimeout) {
                try await Self.open(connector: connector, key: key, random: random, pending: pending,
                                    clock: clock, helloNotAfterMs: helloNotAfterMs)
            }
        } catch {
            // Closes a stream already made; one made later closes itself (`keep` fails).
            await pending.abandon()?.close()
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
    static func open(
        connector: any RunnerConnector,
        key: Redacted<Data>,
        random: any RandomSource,
        pending: PendingStream,
        clock: @escaping @Sendable () -> UInt64,
        helloNotAfterMs: UInt64
    ) async throws -> Connection {
        let stream: any RunnerByteStream
        do {
            stream = try await connector.connect()
        } catch {
            throw JarvisRunnerError.unreachable(String(describing: error))
        }
        // The caller may have given up while the connect was under way: then this stream
        // is nobody's, and it must not keep the runner's only slot.
        guard pending.keep(stream) else {
            await stream.close()
            throw CancellationError()
        }
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
        let connection = Connection(stream: stream, channel: channel, nowMs: clock())
        let hello: RunnerWireReply
        do {
            hello = try await connection.exchange(.hello(protocolVersion: runnerProtocolVersion()), notAfterMs: helloNotAfterMs)
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
        if let error = error as? RequestNotSent { return mapped(error.underlying, authenticated: authenticated) }
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

/// A request found its connection already closed: no byte of it was sent.
struct RequestNotSent: Error, CustomStringConvertible {
    var underlying: any Error
    var description: String { "not sent: \(underlying)" }
}
