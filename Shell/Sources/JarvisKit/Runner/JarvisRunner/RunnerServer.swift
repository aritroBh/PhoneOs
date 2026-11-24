import Foundation
import JarvisCore

/// The runner's side of Jarvis's own runner protocol, transport-free: serves any
/// `PhoneRunner` (on the phone, `XCUITestPhoneRunner` in the runner's UI-test process) to
/// one authenticated client at a time over any `RunnerByteStream`.
///
/// - One connection at a time: a connection accepted while another is served is closed at
///   once. A connection that has not proved the key within `handshakeDeadline` is closed,
///   so a stranger cannot hold the slot for longer than that.
/// - Every runner call has a deadline; past it the client gets an error reply (the call
///   itself may go on: XCUITest calls cannot be interrupted).
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

        public init(
            handshakeDeadline: Duration = .seconds(5),
            readDeadline: Duration = .seconds(30),
            commandDeadline: Duration = .seconds(45),
            runnerName: String = JarvisRunnerDefaults.runnerName
        ) {
            self.handshakeDeadline = handshakeDeadline
            self.readDeadline = readDeadline
            self.commandDeadline = commandDeadline
            self.runnerName = runnerName
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
    private var active: (any RunnerByteStream)?
    private var stopped = false

    public init(
        runner: any PhoneRunner,
        key: Redacted<Data>,
        random: any RandomSource,
        configuration: Configuration = Configuration(),
        onEvent: @escaping @Sendable (Event) -> Void = { _ in }
    ) {
        self.runner = runner
        self.key = key
        self.random = random
        self.configuration = configuration
        self.onEvent = onEvent
    }

    /// Whether a connection is being served.
    public var isServing: Bool { active != nil }

    /// Takes a new connection: served when the server is free, closed at once otherwise.
    /// Returns at once; the connection is served in its own task.
    public func accept(_ connection: any RunnerByteStream) async {
        guard !stopped, active == nil else {
            onEvent(.refusedBusy)
            await connection.close()
            return
        }
        active = connection
        onEvent(.accepted)
        Task { await self.serve(connection) }
    }

    /// Closes the current connection and refuses all later ones.
    public func stop() async {
        stopped = true
        if let active { await active.close() }
    }

    private func finished(_ connection: any RunnerByteStream, reason: String) async {
        await connection.close()
        active = nil
        onEvent(.closed(reason: reason))
    }

    private func serve(_ connection: any RunnerByteStream) async {
        let channel: JarvisRunnerServerChannel
        do {
            channel = try JarvisRunnerServerChannel(key: key.reveal(), entropy: random.bytes(count: 32))
        } catch {
            await finished(connection, reason: "could not open a channel: \(CoreErrorMessage(error).message)")
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
        do {
            while let bytes = try await connection.read() {
                for event in try channel.receive(bytes: bytes) {
                    switch event {
                    case .send(let out):
                        try await connection.write(out)
                    case .authenticated:
                        watchdog.cancel()
                        onEvent(.authenticated)
                    case .request(let id, let request):
                        onEvent(.request(request.operationName))
                        let reply = await answer(request)
                        try await connection.write(try channel.reply(requestId: id, reply: reply))
                    }
                }
            }
            await finished(connection, reason: "the client closed the connection")
        } catch {
            await finished(connection, reason: CoreErrorMessage(error).message)
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
        case .perform(let command, _):
            var limit = configuration.commandDeadline
            if case .wait(let ms) = command { limit += .milliseconds(Int64(clamping: ms)) }
            return await deadlined(.act, limit) {
                .performed(result: try await runner.perform(command))
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
