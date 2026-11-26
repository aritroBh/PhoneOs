import Foundation
import JarvisCore
import Synchronization

/// The runner's side of Jarvis's own runner protocol, transport-free: serves any
/// `PhoneRunner` (on the phone, `XCUITestPhoneRunner` in the runner's UI-test process) to
/// one authenticated client at a time over any `RunnerByteStream`.
///
/// - One connection at a time: a connection accepted while another is served is closed at
///   once. A connection that has not proved the key within `handshakeDeadline` is closed,
///   so a stranger cannot hold the slot for longer than that.
/// - An authenticated connection that sends no request for `idleTimeout` is closed, so a
///   connection its app abandoned does not hold the slot (the client replaces idle
///   connections itself before that).
/// - A request received after its `not_after_ms` (the client's deadline, in the uptime
///   clock both processes share) is refused by the core channel and closes the connection:
///   nothing held back is carried out late.
/// - Every runner call has a deadline (`JarvisRunnerTiming`, inside the client's); past it
///   the client gets an error reply (the call itself may go on: XCUITest calls cannot be
///   interrupted).
/// - A command's `expect` goes to the runner (`perform(_:expect:)`), which checks it next
///   to the gesture; a runner that cannot (`verifiesFrontApp` false) gets no such command:
///   the reply is `stale`, nothing is done.
/// - A failed or closed connection leaves the server ready for the next one.
/// - The core channel does the cryptography and framing; nothing here parses bytes.
public actor RunnerServer {
    public struct Configuration: Sendable {
        /// Time a connection has to prove the key.
        public var handshakeDeadline: Duration
        /// Longest a snapshot, lock read, screenshot or front-app read may take.
        public var readDeadline: Duration
        /// Longest a command may take, plus its own duration for `wait`.
        public var commandDeadline: Duration
        /// The name sent in the hello.
        public var runnerName: String
        /// An authenticated connection with no request for this long is closed.
        public var idleTimeout: Duration

        public init(
            handshakeDeadline: Duration = .seconds(5),
            readDeadline: Duration = JarvisRunnerTiming.serverRead,
            commandDeadline: Duration = JarvisRunnerTiming.serverCommand,
            runnerName: String = JarvisRunnerDefaults.runnerName,
            idleTimeout: Duration = JarvisRunnerTiming.serverIdle
        ) {
            self.handshakeDeadline = handshakeDeadline
            self.readDeadline = readDeadline
            self.commandDeadline = commandDeadline
            self.runnerName = runnerName
            self.idleTimeout = idleTimeout
        }
    }

    /// What happened, for the runner's log and tests (never the key, never tree content).
    public enum Event: Sendable, Equatable {
        case accepted
        case refusedBusy
        case authenticated
        case request(String)
        case closed(reason: String)
    }

    private let runner: any PhoneRunner
    private let key: Redacted<Data>
    private let random: any RandomSource
    private let configuration: Configuration
    private let onEvent: @Sendable (Event) -> Void
    private let clock: @Sendable () -> UInt64
    private var active: (any RunnerByteStream)?
    /// Which accepted connection `active` is (each gets the next number), so a serving task
    /// that ends after its connection was already let go never clears a newer one.
    private var activeNumber: UInt64 = 0
    private var accepted: UInt64 = 0
    /// The idle watch of the active connection, once it authenticated.
    private var activeIdle: IdleWatch?
    private var stopped = false

    /// `clock`: milliseconds of the uptime clock the app reads too (`RunnerUptimeClock.nowMs`;
    /// tests pass their own).
    public init(
        runner: any PhoneRunner,
        key: Redacted<Data>,
        random: any RandomSource,
        configuration: Configuration = Configuration(),
        onEvent: @escaping @Sendable (Event) -> Void = { _ in },
        clock: @escaping @Sendable () -> UInt64 = RunnerUptimeClock.nowMs
    ) {
        self.runner = runner
        self.key = key
        self.random = random
        self.configuration = configuration
        self.onEvent = onEvent
        self.clock = clock
    }

    /// Whether a connection is being served.
    public var isServing: Bool { active != nil }

    /// Takes a new connection: served when the server is free, closed at once otherwise.
    /// Returns at once; the connection is served in its own task.
    public func accept(_ connection: any RunnerByteStream) async {
        // The active connection's peer may have closed it already (the client retires an idle
        // connection and connects again at once) while its serving task has not yet run to
        // see the end: a stream that knows it is closed holds the slot no longer. Without
        // this the new connection was refused as busy whenever the new one won the race to
        // the actor (the "runner connection is closed" flake of the LOW-1 retire test).
        if let current = active, current.isKnownClosed {
            await finished(current, number: activeNumber, reason: "the client closed the connection")
        }
        guard !stopped, active == nil else {
            onEvent(.refusedBusy)
            await connection.close()
            return
        }
        accepted &+= 1
        let number = accepted
        active = connection
        activeNumber = number
        onEvent(.accepted)
        Task { await self.serve(connection, number: number) }
    }

    /// Closes the current connection and refuses all later ones.
    public func stop() async {
        stopped = true
        if let active { await active.close() }
    }

    /// Lets connection `number` go: closes it and, when it still holds the slot, frees the
    /// slot and reports it (once per connection).
    private func finished(_ connection: any RunnerByteStream, number: UInt64, reason: String) async {
        await connection.close()
        guard active != nil, activeNumber == number else { return }
        active = nil
        activeIdle = nil
        onEvent(.closed(reason: reason))
    }

    private func serve(_ connection: any RunnerByteStream, number: UInt64) async {
        let channel: JarvisRunnerServerChannel
        do {
            channel = try JarvisRunnerServerChannel(key: key.reveal(), entropy: random.bytes(count: 32))
        } catch {
            await finished(connection, number: number,
                           reason: "could not open a channel: \(CoreErrorMessage(error).message)")
            return
        }
        // The handshake deadline: closing the connection ends the read below.
        let deadline = configuration.handshakeDeadline
        let watchdog = Task {
            try? await Task.sleep(for: deadline)
            guard !Task.isCancelled else { return }
            await connection.close()
        }
        defer { watchdog.cancel() }
        let idle = IdleWatch(nowMs: clock())
        var idleWatchdog: Task<Void, Never>?
        defer { idleWatchdog?.cancel() }
        do {
            while let bytes = try await connection.read() {
                for event in try channel.receive(bytes: bytes, nowMs: clock()) {
                    switch event {
                    case .send(let out):
                        try await connection.write(out)
                    case .authenticated:
                        watchdog.cancel()
                        onEvent(.authenticated)
                        idle.busy(false, nowMs: clock())
                        if activeNumber == number { activeIdle = idle }
                        idleWatchdog = watchIdle(idle)
                    case .request(let id, _, let request): // not_after: checked by the core channel on receipt
                        idle.busy(true, nowMs: clock())
                        onEvent(.request(request.operationName))
                        let reply = await answer(request)
                        try await connection.write(try channel.reply(requestId: id, reply: reply))
                        idle.busy(false, nowMs: clock())
                    }
                }
            }
            let reason = idle.fired ? "no request for \(configuration.idleTimeout)" : "the client closed the connection"
            await finished(connection, number: number, reason: reason)
        } catch {
            await finished(connection, number: number, reason: CoreErrorMessage(error).message)
        }
    }

    /// Tracks the last request of one authenticated connection, in the server's `clock`.
    final class IdleWatch: Sendable {
        private struct State {
            var busy = false
            var sinceMs: UInt64
            var fired = false
        }

        private let state: Mutex<State>

        init(nowMs: UInt64) {
            state = Mutex(State(sinceMs: nowMs))
        }

        func busy(_ busy: Bool, nowMs: UInt64) {
            state.withLock {
                $0.busy = busy
                $0.sinceMs = nowMs
            }
        }

        /// Milliseconds until the connection counts as idle at `nowMs`; `nil` once it does
        /// (and remembers it). A request being carried out never counts as idle.
        func remainingMs(_ limitMs: UInt64, nowMs: UInt64) -> UInt64? {
            state.withLock { s in
                if s.busy { return limitMs }
                let idleMs = nowMs &- min(nowMs, s.sinceMs)
                if idleMs < limitMs { return limitMs - idleMs }
                s.fired = true
                return nil
            }
        }

        var fired: Bool { state.withLock { $0.fired } }
    }

    /// Closes the active connection when it has gone `idleTimeout` (by `clock`) without a
    /// request. The watchdog calls it; tests call it after moving their clock.
    func checkIdle() async {
        guard let idle = activeIdle, let connection = active else { return }
        if idle.remainingMs(configuration.idleTimeout.wholeMilliseconds, nowMs: clock()) == nil {
            await connection.close()
        }
    }

    /// Sleeps until the connection may have gone idle, then checks (by `clock`).
    private func watchIdle(_ idle: IdleWatch) -> Task<Void, Never> {
        let limitMs = configuration.idleTimeout.wholeMilliseconds
        let clock = clock
        return Task { [weak self] in
            while !Task.isCancelled {
                let waitMs = idle.remainingMs(limitMs, nowMs: clock()) ?? 0
                if waitMs > 0 {
                    try? await Task.sleep(for: .milliseconds(Int64(clamping: waitMs)))
                    continue
                }
                await self?.checkIdle()
                if idle.fired || self == nil { return }
            }
        }
    }

    /// Carries out one request against the runner, within its deadline.
    private func answer(_ request: RunnerWireRequest) async -> RunnerWireReply {
        let runner = runner
        let name = configuration.runnerName
        switch request {
        case .hello:
            return .hello(protocolVersion: runnerProtocolVersion(), runner: name)
        case .snapshot(let includeScreenshot):
            return await deadlined(.observe, configuration.readDeadline) {
                let observation = try await runner.snapshot(includeScreenshot: includeScreenshot)
                guard let wire = RunnerWireSnapshot(observation) else {
                    return .error(message: "the runner read a text tree; Jarvis's runner serves typed trees only")
                }
                return .snapshot(snapshot: wire)
            }
        case .perform(let command, let expect):
            // A runner that cannot check `expect` next to the gesture never gets a command
            // that carries one.
            if let expect, !runner.verifiesFrontApp {
                return .performed(result: .staleUnreadable(expected: expect.bundleId))
            }
            var limit = configuration.commandDeadline
            if case .wait(let ms) = command { limit += .milliseconds(Int64(clamping: ms)) }
            return await deadlined(.act, limit) {
                .performed(result: try await runner.perform(command, expect: expect))
            }
        case .lockState:
            return await deadlined(.lockState, configuration.readDeadline) {
                .lockState(state: RunnerWireLockState(try await runner.lockState()))
            }
        case .viewerScreenshot:
            return await deadlined(.viewerScreenshot, configuration.readDeadline) {
                .viewerScreenshot(screenshot: try await runner.viewerScreenshot().map(RunnerWireScreenshot.init))
            }
        case .frontBundleId:
            return await deadlined(.observe, configuration.readDeadline) {
                .frontBundleId(bundleId: try await runner.frontBundleId())
            }
        }
    }

    private func deadlined(
        _ kind: IOKind,
        _ limit: Duration,
        _ operation: @escaping @Sendable () async throws -> RunnerWireReply
    ) async -> RunnerWireReply {
        do {
            return try await IORace<RunnerWireReply>().run(kind: kind, timeout: limit, operation)
        } catch {
            return .error(message: String(describing: error))
        }
    }
}
