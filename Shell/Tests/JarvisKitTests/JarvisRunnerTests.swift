import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// Jarvis's own runner protocol in the shell: `JarvisRunnerClient` (a `PhoneRunner`) and
// `RunnerServer` over in-memory pipes, with the real core channels in between. The
// protocol's cryptographic proof is in core/jarvis-runner/tests; these check the shell
// keeps its promises: authenticate before trusting, one client, deadlines, reconnect,
// errors that keep their meaning.

private let key = Data(repeating: 0x5A, count: 32)
private let otherKey = Data(repeating: 0xA5, count: 32)

/// A runner serving a fixed typed tree, recording every call.
private final class TypedFakeRunner: PhoneRunner {
    struct State {
        var calls: [String] = []
        var commands: [RunnerCommand] = []
        var snapshotDelay: Duration?
        var result = RunnerResult(ok: true, error: nil)
        var lock: DeviceLockState = .unlocked
        var front: String? = "dev.jarvis.app"
        var shot: CapturedScreenshot?
        var failSnapshot: String?
    }

    let state = Mutex(State())
    let root: UiNode
    let label: String
    /// Like the XCUITest runner: checks `expect` next to the gesture.
    let verifiesFrontApp: Bool

    init(label: String = "Playground", children: Int = 3, verifies: Bool = true) {
        self.label = label
        root = TypedFakeRunner.tree(label: label, children: children)
        verifiesFrontApp = verifies
    }

    static func node(_ role: Role, _ label: String, _ y: Double, children: [UiNode] = []) -> UiNode {
        UiNode(role: role, label: label, value: nil, identifier: "id.\(label)", placeholder: nil,
               frame: Rect(x: 0, y: y, width: 402, height: 44), enabled: true, visible: true, hittable: true,
               focused: false, selected: false, secure: false, children: children)
    }

    static func tree(label: String, children: Int) -> UiNode {
        node(.application, label, 0, children: (0..<children).map { node(.button, "Row \($0)", Double(100 + 44 * $0)) })
    }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let (delay, failure, shot) = state.withLock { s -> (Duration?, String?, CapturedScreenshot?) in
            s.calls.append("snapshot")
            defer { s.snapshotDelay = nil }
            return (s.snapshotDelay, s.failSnapshot, s.shot)
        }
        if let delay { try await Task.sleep(for: delay) }
        if let failure { throw JarvisRunnerError.runnerFailed(failure) }
        return RunnerObservation(
            root: root, appBundleId: "dev.jarvis.app", appName: "Jarvis", screenSize: Size(width: 402, height: 874),
            keyboardVisible: false, orientation: .portrait, screenshot: includeScreenshot ? shot : nil,
            deviceLocked: false, notes: ["typed"], lockState: .unlocked
        )
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        try await perform(command, expect: nil)
    }

    /// The check and the "gesture" in one locked step, as the XCUITest runner does them in
    /// one main-actor step.
    func perform(_ command: RunnerCommand, expect: CommandExpect?) async throws -> RunnerResult {
        state.withLock { s in
            if verifiesFrontApp, let refusal = FrontAppGuard.refusalNextToGesture(expect, front: s.front) {
                s.calls.append("refused")
                return refusal
            }
            s.calls.append("perform")
            s.commands.append(command)
            return s.result
        }
    }

    func lockState() async throws -> DeviceLockState {
        state.withLock { s in
            s.calls.append("lockState")
            return s.lock
        }
    }

    func viewerScreenshot() async throws -> CapturedScreenshot? {
        state.withLock { s in
            s.calls.append("viewerScreenshot")
            return s.shot
        }
    }

    func frontBundleId() async throws -> String? {
        state.withLock { s in
            s.calls.append("frontBundleId")
            return s.front
        }
    }

    var calls: [String] { state.withLock { $0.calls } }
}

/// Connects to a `RunnerServer` through a fresh in-memory pipe per connection.
private final class PipeConnector: RunnerConnector {
    let server: RunnerServer
    /// The client ends handed out, in order (tests read what was written, or drop one).
    let ends = Mutex<[InMemoryRunnerStream]>([])
    let serverEnds = Mutex<[InMemoryRunnerStream]>([])

    init(_ server: RunnerServer) {
        self.server = server
    }

    func connect() async throws -> any RunnerByteStream {
        let (a, b) = InMemoryRunnerPipe.make()
        ends.withLock { $0.append(a) }
        serverEnds.withLock { $0.append(b) }
        await server.accept(b)
        return a
    }
}

/// Nothing listens.
private struct NoRunner: RunnerConnector {
    struct Refused: Error {}
    func connect() async throws -> any RunnerByteStream { throw Refused() }
}

private func makeServer(
    _ runner: any PhoneRunner,
    key serverKey: Data = key,
    configuration: RunnerServer.Configuration = RunnerServer.Configuration(),
    events log: EventLog? = nil,
    clock: @escaping @Sendable () -> UInt64 = RunnerUptimeClock.nowMs
) -> RunnerServer {
    RunnerServer(runner: runner, key: Redacted(serverKey), random: GeneratorRandomSource(),
                 configuration: configuration, onEvent: { e in log?.events.withLock { $0.append(e) } }, clock: clock)
}

private final class EventLog: Sendable {
    let events = Mutex<[RunnerServer.Event]>([])
}

private func makeClient(
    _ connector: any RunnerConnector,
    key clientKey: Data? = key,
    configuration: JarvisRunnerClient.Configuration = JarvisRunnerClient.Configuration(),
    clock: @escaping @Sendable () -> UInt64 = RunnerUptimeClock.nowMs
) -> JarvisRunnerClient {
    let source: any RunnerKeySource = clientKey.map { FixedRunnerKeySource($0) } ?? NoKey()
    return JarvisRunnerClient(connector: connector, keySource: source, random: GeneratorRandomSource(),
                              configuration: configuration, clock: clock)
}

/// A clock tests move by hand (ms).
private final class RunnerTestClock: Sendable {
    let now = Mutex<UInt64>(1_000_000)
    func read() -> UInt64 { now.withLock { $0 } }
    func advance(_ ms: UInt64) { now.withLock { $0 += ms } }
    var function: @Sendable () -> UInt64 { { [self] in read() } }
}

/// Refuses the connections listed in `refuse` (0-based), otherwise connects to `server`.
private final class FlakyConnector: RunnerConnector {
    let inner: PipeConnector
    let attempts = Mutex(0)
    let refuse: Set<Int>

    init(_ server: RunnerServer, refuse: Set<Int>) {
        inner = PipeConnector(server)
        self.refuse = refuse
    }

    func connect() async throws -> any RunnerByteStream {
        let n = attempts.withLock { a -> Int in
            defer { a += 1 }
            return a
        }
        if refuse.contains(n) { throw NoRunner.Refused() }
        return try await inner.connect()
    }
}

/// Hands out a pipe end only when `release()` is called (a connect that hangs).
private final class HangingConnector: RunnerConnector {
    private let waiter = Mutex<CheckedContinuation<Void, Never>?>(nil)
    private let released = Mutex(false)
    let ends = Mutex<[InMemoryRunnerStream]>([])
    let farEnds = Mutex<[InMemoryRunnerStream]>([])

    func connect() async throws -> any RunnerByteStream {
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            let go = released.withLock { $0 }
            if go { c.resume() } else { waiter.withLock { $0 = c } }
        }
        let (a, b) = InMemoryRunnerPipe.make()
        ends.withLock { $0.append(a) }
        farEnds.withLock { $0.append(b) }
        return a
    }

    func release() {
        released.withLock { $0 = true }
        waiter.withLock { w in
            w?.resume()
            w = nil
        }
    }
}

private struct NoKey: RunnerKeySource {
    func runnerKey() throws -> Redacted<Data>? { nil }
}

/// Waits (bounded) until `condition` holds.
private func eventually(_ what: String, tries: Int = 200, _ condition: @Sendable () async -> Bool) async throws {
    for _ in 0..<tries {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("timed out waiting for: \(what)")
}

/// A runner on the key that answers the hello, then closes the connection on any other
/// request (after reading it whole): a runner that dies mid-call.
private final class DroppingRunner: Sendable {
    let requests = Mutex<[String]>([])
    let connections = Mutex(0)

    func serve(_ far: InMemoryRunnerStream) {
        connections.withLock { $0 += 1 }
        Task {
            guard let channel = try? JarvisRunnerServerChannel(key: key, entropy: GeneratorRandomSource().bytes(count: 32)) else { return }
            while let bytes = try? await far.read() {
                guard let events = try? channel.receive(bytes: bytes, nowMs: RunnerUptimeClock.nowMs()) else { break }
                for event in events {
                    switch event {
                    case .send(let out):
                        try? await far.write(out)
                    case .authenticated:
                        break
                    case .request(let id, _, let request):
                        if case .hello = request {
                            let reply = RunnerWireReply.hello(protocolVersion: runnerProtocolVersion(), runner: "dropping")
                            if let out = try? channel.reply(requestId: id, reply: reply) { try? await far.write(out) }
                        } else {
                            requests.withLock { $0.append(request.operationName) }
                            await far.close()
                            return
                        }
                    }
                }
            }
            await far.close()
        }
    }
}

/// A client end that sends the first half of its `failWrite`-th write (1-based) and then
/// fails with an error that is not `RunnerStreamClosed` (bytes may have left).
private final class HalfWritingStream: RunnerByteStream {
    struct Broken: Error {}
    let inner: InMemoryRunnerStream
    let failWrite: Int?
    let writes = Mutex(0)

    init(_ inner: InMemoryRunnerStream, failWrite: Int?) {
        self.inner = inner
        self.failWrite = failWrite
    }

    func write(_ bytes: Data) async throws {
        let n = writes.withLock { w -> Int in
            w += 1
            return w
        }
        if n == failWrite {
            try await inner.write(bytes.prefix(bytes.count / 2))
            await inner.close()
            throw Broken()
        }
        try await inner.write(bytes)
    }

    func read() async throws -> Data? { try await inner.read() }
    func close() async { await inner.close() }
    var isKnownClosed: Bool { inner.isKnownClosed }
}

/// Hands each new connection's far end to `serve`, counting connections.
private final class RawConnector: RunnerConnector {
    let serveFar: @Sendable (InMemoryRunnerStream) -> Void
    let failWrite: Int?
    let made = Mutex(0)

    init(failWrite: Int? = nil, _ serve: @escaping @Sendable (InMemoryRunnerStream) -> Void) {
        serveFar = serve
        self.failWrite = failWrite
    }

    var count: Int { made.withLock { $0 } }

    func connect() async throws -> any RunnerByteStream {
        let (a, b) = InMemoryRunnerPipe.make()
        let n = made.withLock { m -> Int in
            m += 1
            return m
        }
        serveFar(b)
        return HalfWritingStream(a, failWrite: n == 1 ? failWrite : nil)
    }
}

@Suite("Jarvis runner protocol (shell)", .timeLimit(.minutes(1)))
struct JarvisRunnerTests {
    @Test("the whole PhoneRunner surface crosses intact: typed tree, results with their failures, lock, front app, screenshots")
    func surfaceRoundTrip() async throws {
        let fake = TypedFakeRunner()
        let shot = CapturedScreenshot(data: Data((0..<5_000).map { UInt8(truncatingIfNeeded: $0) }), mimeType: "image/png",
                                      pixelWidth: 1206, pixelHeight: 2622, scale: 3)
        fake.state.withLock { $0.shot = shot }
        let client = makeClient(PipeConnector(makeServer(fake)))

        let observation = try await client.snapshot(includeScreenshot: true)
        #expect(observation.root == fake.root, "the typed tree, not text")
        #expect(observation.source == nil)
        #expect(observation.screenshot == shot)
        #expect(observation.lockState == .unlocked && observation.isKnownUnlocked)
        #expect(observation.notes == ["typed"])
        #expect(try await client.snapshot(includeScreenshot: false).screenshot == nil)

        let tap = RunnerCommand.tap(x: 10, y: 20)
        #expect(try await client.perform(tap) == RunnerResult(ok: true, error: nil))
        for result in [RunnerResult.deviceLocked(), RunnerResult.stale(front: "a", expected: "b"),
                       RunnerResult(ok: false, error: "no", failure: .other)] {
            fake.state.withLock { $0.result = result }
            #expect(try await client.perform(.typeText(text: "héllo")) == result)
        }
        #expect(fake.state.withLock { $0.commands } == [tap, .typeText(text: "héllo"), .typeText(text: "héllo"), .typeText(text: "héllo")])

        for lock in [DeviceLockState.locked, .unlocked, .unknown] {
            fake.state.withLock { $0.lock = lock }
            #expect(try await client.lockState() == lock)
        }
        #expect(try await client.frontBundleId() == "dev.jarvis.app")
        fake.state.withLock { $0.front = nil }
        #expect(try await client.frontBundleId() == nil)
        #expect(try await client.viewerScreenshot() == shot)
        fake.state.withLock { $0.shot = nil }
        #expect(try await client.viewerScreenshot() == nil)
    }

    @Test("a large tree and screenshot cross intact")
    func largeSnapshot() async throws {
        let fake = TypedFakeRunner(children: 5_000)
        fake.state.withLock { $0.shot = CapturedScreenshot(data: Data(repeating: 7, count: 3_000_000), mimeType: "image/png", pixelWidth: 1, pixelHeight: 1) }
        let client = makeClient(PipeConnector(makeServer(fake)))
        let observation = try await client.snapshot(includeScreenshot: true)
        #expect(observation.root == fake.root)
        #expect(observation.screenshot?.data.count == 3_000_000)
    }

    @Test("the runner's own failure is a runner error, never a screen")
    func runnerFailure() async throws {
        let fake = TypedFakeRunner()
        fake.state.withLock { $0.failSnapshot = "the app is not running" }
        let client = makeClient(PipeConnector(makeServer(fake)))
        await #expect(throws: JarvisRunnerError.self) { try await client.snapshot(includeScreenshot: false) }
        do {
            _ = try await client.snapshot(includeScreenshot: false)
        } catch let error as JarvisRunnerError {
            guard case .runnerFailed(let message) = error else { Issue.record("\(error)"); return }
            #expect(message.contains("the app is not running"))
        }
    }

    @Test("a client with the wrong key gets nothing: the runner is never called")
    func wrongKeyClient() async throws {
        let fake = TypedFakeRunner()
        let client = makeClient(PipeConnector(makeServer(fake)), key: otherKey)
        await #expect(throws: JarvisRunnerError.unauthenticated) { try await client.snapshot(includeScreenshot: false) }
        await #expect(throws: JarvisRunnerError.unauthenticated) { try await client.perform(.tap(x: 1, y: 1)) }
        #expect(fake.calls.isEmpty)
    }

    @Test("a squatting server with another key is treated as absent: no request reaches it, nothing it serves is used")
    func squatter() async throws {
        let forged = TypedFakeRunner(label: "FORGED")
        let squatter = makeServer(forged, key: otherKey)
        let client = makeClient(PipeConnector(squatter))
        await #expect(throws: JarvisRunnerError.unauthenticated) { try await client.snapshot(includeScreenshot: false) }
        await #expect(throws: JarvisRunnerError.unauthenticated) { try await client.perform(.typeText(text: "secret")) }
        #expect(forged.calls.isEmpty, "the client sent a request to an unauthenticated server")
        #expect(!client.isConnected)
    }

    @Test("no key on the phone: not provisioned, nothing is sent")
    func notProvisioned() async throws {
        let fake = TypedFakeRunner()
        let connector = PipeConnector(makeServer(fake))
        let client = makeClient(connector, key: nil)
        await #expect(throws: JarvisRunnerError.notProvisioned) { try await client.lockState() }
        #expect(connector.ends.withLock { $0.isEmpty })
    }

    @Test("nothing listening is unreachable")
    func unreachable() async throws {
        let client = makeClient(NoRunner())
        do {
            _ = try await client.lockState()
            Issue.record("no error")
        } catch let error as JarvisRunnerError {
            guard case .unreachable = error else { Issue.record("\(error)"); return }
        }
    }

    @Test("a second client is refused while one is served; the first keeps working")
    func secondClientRefused() async throws {
        let fake = TypedFakeRunner()
        let events = EventLog()
        let server = makeServer(fake, events: events)
        let first = makeClient(PipeConnector(server))
        #expect(try await first.lockState() == .unlocked)
        let second = makeClient(PipeConnector(server))
        do {
            _ = try await second.lockState()
            Issue.record("the second client was served")
        } catch let error as JarvisRunnerError {
            guard case .unreachable = error else { Issue.record("\(error)"); return }
        }
        #expect(events.events.withLock { $0.contains(.refusedBusy) })
        #expect(try await first.frontBundleId() == "dev.jarvis.app")
    }

    @Test("a connection that does not prove the key in time is closed and the slot frees")
    func handshakeDeadline() async throws {
        let fake = TypedFakeRunner()
        let server = makeServer(fake, configuration: RunnerServer.Configuration(handshakeDeadline: .milliseconds(200)))
        let (silent, serverEnd) = InMemoryRunnerPipe.make()
        await server.accept(serverEnd)
        #expect(await server.isServing)
        #expect(try await silent.read() == nil, "the server closed the silent connection")
        try await eventually("the slot frees") { await !server.isServing }
        let client = makeClient(PipeConnector(server))
        #expect(try await client.lockState() == .unlocked)
    }

    @Test("past the server's deadline the client hears an error; past the client's it drops and reconnects")
    func deadlines() async throws {
        let fake = TypedFakeRunner()
        fake.state.withLock { $0.snapshotDelay = .seconds(2) }
        let server = makeServer(fake, configuration: RunnerServer.Configuration(readDeadline: .milliseconds(200)))
        let client = makeClient(PipeConnector(server))
        do {
            _ = try await client.snapshot(includeScreenshot: false)
            Issue.record("no error")
        } catch let error as JarvisRunnerError {
            guard case .runnerFailed(let message) = error else { Issue.record("\(error)"); return }
            #expect(message.contains("timed out"))
        }

        let slow = TypedFakeRunner()
        slow.state.withLock { $0.snapshotDelay = .seconds(2) }
        let connector = PipeConnector(makeServer(slow))
        let impatient = makeClient(connector, configuration: JarvisRunnerClient.Configuration(readTimeout: .milliseconds(300)))
        await #expect(throws: JarvisRunnerError.timedOut(operation: "snapshot", after: .milliseconds(300))) {
            try await impatient.snapshot(includeScreenshot: false)
        }
        #expect(!impatient.isConnected, "a timed-out connection is dropped")
        // The server is busy with the old connection until its runner call ends; the next
        // call connects again once it is free.
        try await eventually("a reconnect succeeds") { (try? await impatient.lockState()) == .unlocked }
        #expect(connector.ends.withLock { $0.count } >= 2)
    }

    @Test("after a dropped connection the next call connects again")
    func reconnectAfterDrop() async throws {
        let fake = TypedFakeRunner()
        let connector = PipeConnector(makeServer(fake))
        let client = makeClient(connector)
        #expect(try await client.lockState() == .unlocked)
        await connector.serverEnds.withLock { $0[0] }.close()
        // The call that meets the dead connection fails as unreachable (never a value);
        // the one after it connects again.
        var outcomes: [String] = []
        for _ in 0..<2 {
            do {
                outcomes.append("\(try await client.lockState())")
            } catch let error as JarvisRunnerError {
                guard case .unreachable = error else { Issue.record("\(error)"); return }
                outcomes.append("unreachable")
            }
        }
        #expect(outcomes.last == "unlocked")
        #expect(connector.ends.withLock { $0.count } == 2)
    }

    @Test("a recorded session replayed into a new connection carries out nothing")
    func wholeSessionReplay() async throws {
        let fake = TypedFakeRunner()
        let events = EventLog()
        let server = makeServer(fake, events: events)
        let connector = PipeConnector(server)
        let client = makeClient(connector)
        _ = try await client.perform(.tap(x: 5, y: 5))
        let recording = connector.ends.withLock { $0[0] }.written.withLock { $0 }
        await client.disconnect()
        try await eventually("the first connection is done") { await !server.isServing }

        let (attacker, serverEnd) = InMemoryRunnerPipe.make()
        await server.accept(serverEnd)
        try await attacker.write(recording)
        // The server answers the replayed hello, then refuses the replayed finish.
        while try await attacker.read() != nil {}
        #expect(fake.state.withLock { $0.commands } == [.tap(x: 5, y: 5)], "the replayed tap was carried out")
        #expect(events.events.withLock { $0.filter { $0 == .authenticated }.count } == 1)
    }

    @Test("random bytes into both core channels: no crash, bounded time, no request or reply")
    func randomBytes() throws {
        var generator = SystemRandomNumberGenerator()
        let start = ContinuousClock.now
        for round in 0..<500 {
            let length = Int.random(in: 0..<400, using: &generator)
            var bytes = Data((0..<length).map { _ in UInt8.random(in: 0...255, using: &generator) })
            if round % 2 == 0, bytes.count > 5 {
                let n = UInt32(bytes.count - 4).bigEndian
                withUnsafeBytes(of: n) { bytes.replaceSubrange(0..<4, with: $0) }
            }
            let server = try JarvisRunnerServerChannel(key: key, entropy: GeneratorRandomSource().bytes(count: 32))
            let client = try JarvisRunnerClientChannel(key: key, entropy: GeneratorRandomSource().bytes(count: 32))
            _ = try client.start()
            if let events = try? server.receive(bytes: bytes, nowMs: RunnerUptimeClock.nowMs()) {
                #expect(events.allSatisfy { if case .send = $0 { true } else { false } })
            }
            if let events = try? client.receive(bytes: bytes) {
                #expect(events.isEmpty)
            }
        }
        #expect(ContinuousClock.now - start < .seconds(30))
    }

    @Test("the key: adopted from the launch environment once, read from the store after, malformed values refused without echo")
    func keyProvisioning() throws {
        let store = InMemorySecretStore()
        let source = ProvisionedRunnerKeySource(store: store)
        #expect(try source.runnerKey() == nil)
        #expect(try source.adopt(environment: [:]) == false)
        let fresh = try RunnerKey.generate(GeneratorRandomSource())
        let environment = ProvisionedRunnerKeySource.environment(for: fresh)
        #expect(try source.adopt(environment: environment))
        #expect(try source.runnerKey() == fresh)
        let bad = "not-a-key-AAAA"
        do {
            try source.adopt(environment: [ProvisionedRunnerKeySource.environmentVariable: bad])
            Issue.record("a malformed key was adopted")
        } catch {
            #expect(!"\(error)".contains(bad))
        }
        #expect(try source.runnerKey() == fresh, "a refused value leaves the stored key")
        #expect("\(fresh)" == "<redacted>")
    }

    @Test("settings: the runner choice defaults to the text runner; an unknown value reads as it")
    func settingsRunnerKind() throws {
        #expect(JarvisSettings().runnerKind == .textRunner)
        var settings = JarvisSettings()
        settings.runnerKind = .jarvis
        let data = try JSONEncoder().encode(settings)
        #expect(try JSONDecoder().decode(JarvisSettings.self, from: data).runnerKind == .jarvis)
        let unknown = #"{"runner_kind":"telepathy"}"#
        #expect(try JSONDecoder().decode(JarvisSettings.self, from: Data(unknown.utf8)).runnerKind == .textRunner)
    }

    // MARK: - Twenty-first review

    @Test("timeouts: server deadline < client deadline < driver deadline, from one place")
    func timeoutLayering() {
        let driver = DriverConfiguration()
        let client = JarvisRunnerClient.Configuration()
        let server = RunnerServer.Configuration()
        #expect(server.readDeadline < client.readTimeout && client.readTimeout < driver.observeTimeout)
        #expect(server.commandDeadline < client.commandTimeout && client.commandTimeout < driver.actTimeout)
        #expect(client.connectTimeout < client.readTimeout)
        #expect(client.idleRetire < server.idleTimeout)
        // The driver's deadlines were not lengthened.
        #expect(driver.observeTimeout == .seconds(20) && driver.actTimeout == .seconds(20))
    }

    @Test("MED-1: the expectation travels with the command; another app in front at the runner is stale, nothing done")
    func expectationMismatch() async throws {
        let fake = TypedFakeRunner()
        fake.state.withLock { $0.front = "com.apple.springboard" }
        let client = makeClient(PipeConnector(makeServer(fake)))
        let result = try await client.perform(.tap(x: 1, y: 2), expect: CommandExpect(bundleId: "dev.jarvis.app"))
        #expect(!result.ok && result.failure == .stale)
        #expect(fake.state.withLock { $0.commands }.isEmpty, "the tap was carried out")
        // The same expectation with the right app in front runs.
        fake.state.withLock { $0.front = "DEV.jarvis.app" }
        #expect(try await client.perform(.tap(x: 1, y: 2), expect: CommandExpect(bundleId: "dev.jarvis.app")).ok)
        #expect(fake.state.withLock { $0.commands } == [.tap(x: 1, y: 2)])
    }

    @Test("MED-1: the runner cannot read the front app: stale, nothing done")
    func expectationUnreadable() async throws {
        let fake = TypedFakeRunner()
        fake.state.withLock { $0.front = nil }
        let client = makeClient(PipeConnector(makeServer(fake)))
        let result = try await client.perform(.typeText(text: "secret"), expect: CommandExpect(bundleId: "dev.jarvis.app"))
        #expect(!result.ok && result.failure == .stale)
        #expect(fake.state.withLock { $0.commands }.isEmpty)
    }

    @Test("MED-1: a served runner that cannot check expectations never gets a command carrying one")
    func nonVerifyingRunnerRefusesExpectations() async throws {
        let fake = TypedFakeRunner(verifies: false)
        let client = makeClient(PipeConnector(makeServer(fake)))
        let result = try await client.perform(.tap(x: 1, y: 2), expect: CommandExpect(bundleId: "dev.jarvis.app"))
        #expect(!result.ok && result.failure == .stale)
        #expect(fake.state.withLock { $0.commands }.isEmpty)
        // Commands without an expectation still run.
        #expect(try await client.perform(.launchApp(bundleId: "dev.jarvis.app")).ok)
        #expect(fake.state.withLock { $0.commands } == [.launchApp(bundleId: "dev.jarvis.app")])
    }

    @Test("MED-1: the channel down during the front-app read refuses the command: the error surfaces, nothing is sent")
    func channelDownDuringFrontRead() async throws {
        let fake = TypedFakeRunner()
        let connector = FlakyConnector(makeServer(fake), refuse: [0])
        let client = makeClient(connector)
        #expect(client.verifiesFrontApp)
        await #expect(throws: JarvisRunnerError.self) {
            _ = try await FrontAppGuard().perform(.tap(x: 1, y: 2), expect: CommandExpect(bundleId: "dev.jarvis.app"), on: client)
        }
        #expect(connector.attempts.withLock { $0 } == 1, "the command was sent after the failed read")
        #expect(fake.calls.isEmpty)
    }

    @Test("MED-1: a keyless peer cycling the slot cannot cause an unchecked command")
    func cyclingPeerCannotUncheck() async throws {
        // The front read meets a refused connect (a stranger held the slot); the perform
        // meets a free slot. The guard refuses; and even a command sent anyway (as a guard
        // that took the failed read for "unknown" would) is checked at the runner.
        let fake = TypedFakeRunner()
        fake.state.withLock { $0.front = "com.apple.springboard" }
        let connector = FlakyConnector(makeServer(fake), refuse: [0])
        let client = makeClient(connector)
        let expect = CommandExpect(bundleId: "dev.jarvis.app")
        await #expect(throws: JarvisRunnerError.self) {
            _ = try await FrontAppGuard().perform(.tap(x: 1, y: 2), expect: expect, on: client)
        }
        let sentAnyway = try await client.perform(.tap(x: 1, y: 2), expect: expect)
        #expect(sentAnyway.failure == .stale)
        #expect(fake.state.withLock { $0.commands }.isEmpty, "an unchecked command ran")
    }

    @Test("MED-1: runners that do not verify behave as before: an unreadable front app sends the command once noted")
    func nonVerifyingGuardUnchanged() async throws {
        struct Throwing: PhoneRunner {
            let inner = FakePhone()
            func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation { try await inner.snapshot(includeScreenshot: includeScreenshot) }
            func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }
            func frontBundleId() async throws -> String? { throw JarvisRunnerError.unreachable("x") }
        }
        let phone = Throwing()
        #expect(!phone.verifiesFrontApp)
        let out = try await FrontAppGuard().perform(.tap(x: 1, y: 2), expect: CommandExpect(bundleId: "dev.jarvis.app"), on: phone)
        #expect(out.result.ok && out.note == FrontAppGuard.unknownNote)
        #expect(phone.inner.commands.count == 1)
    }

    @Test("LOW-1: a connect that completes after the deadline closes its stream and frees the slot")
    func lateConnectIsClosed() async throws {
        let connector = HangingConnector()
        let client = makeClient(connector, configuration: JarvisRunnerClient.Configuration(connectTimeout: .milliseconds(100)))
        await #expect(throws: JarvisRunnerError.timedOut(operation: "connect", after: .milliseconds(100))) {
            try await client.lockState()
        }
        connector.release()
        try await eventually("the late stream is handed out") { connector.farEnds.withLock { !$0.isEmpty } }
        let far = connector.farEnds.withLock { $0[0] }
        // The far end (the runner's side) sees the connection closed without a byte.
        #expect(try await far.read() == nil)
        #expect(connector.ends.withLock { $0[0] }.isClosed)
        #expect(!client.isConnected)
    }

    @Test("LOW-1: an authenticated connection idle past the server's limit is closed (by the server's clock, not by real time)")
    func idleTimeout() async throws {
        let fake = TypedFakeRunner()
        let events = EventLog()
        let clock = RunnerTestClock()
        // The default limit (120 s) on a clock the test moves: no real-time margin.
        let server = makeServer(fake, events: events, clock: clock.function)
        let (end, serverEnd) = InMemoryRunnerPipe.make()
        await server.accept(serverEnd)
        // Authenticate by hand, then say nothing.
        let channel = try JarvisRunnerClientChannel(key: key, entropy: GeneratorRandomSource().bytes(count: 32))
        try await end.write(try channel.start())
        var authenticated = false
        while !authenticated, let bytes = try await end.read() {
            for event in try channel.receive(bytes: bytes) {
                if case .send(let out) = event { try await end.write(out) }
                if case .authenticated = event { authenticated = true }
            }
        }
        #expect(authenticated)
        // The server marks the connection authenticated as it handles the finish, which can
        // come after the client's own event: wait for that (no time limit involved).
        try await eventually("the server saw the client's proof", tries: 3_000) {
            events.events.withLock { $0.contains(.authenticated) }
        }
        clock.advance(JarvisRunnerTiming.serverIdle.wholeMilliseconds - 1)
        await server.checkIdle()
        #expect(!end.isClosed, "closed before the limit")
        #expect(await server.isServing)
        clock.advance(1)
        await server.checkIdle()
        #expect(try await end.read() == nil, "the idle connection was not closed")
        try await eventually("the slot frees", tries: 3_000) { await !server.isServing }
        #expect(events.events.withLock { $0.contains { if case .closed(let r) = $0 { r.contains("no request") } else { false } } })
    }

    @Test("LOW-1: the client retires its own idle connection before the server would (one clock for both ends)")
    func clientRetiresIdleConnection() async throws {
        let fake = TypedFakeRunner()
        let clock = RunnerTestClock()
        let connector = PipeConnector(makeServer(fake, clock: clock.function))
        let client = makeClient(connector, configuration: JarvisRunnerClient.Configuration(idleRetire: .seconds(100)), clock: clock.function)
        #expect(try await client.lockState() == .unlocked)
        clock.advance(99_000)
        #expect(try await client.lockState() == .unlocked)
        #expect(connector.ends.withLock { $0.count } == 1)
        clock.advance(101_000)
        #expect(try await client.lockState() == .unlocked)
        #expect(connector.ends.withLock { $0.count } == 2, "an idle connection was reused")
    }

    @Test("retry: a command whose connection the runner already closed (idle cut) is sent once on a fresh connection")
    func closedBeforeWriteIsRetried() async throws {
        let fake = TypedFakeRunner()
        let clock = RunnerTestClock()
        let server = makeServer(fake, clock: clock.function)
        let connector = PipeConnector(server)
        // The client would keep using the connection (idleRetire far away); the server cuts it.
        let client = makeClient(connector, configuration: JarvisRunnerClient.Configuration(idleRetire: .seconds(10_000)), clock: clock.function)
        #expect(try await client.lockState() == .unlocked)
        clock.advance(JarvisRunnerTiming.serverIdle.wholeMilliseconds)
        await server.checkIdle()
        let first = connector.ends.withLock { $0[0] }
        try await eventually("the cut reaches the client's end", tries: 3_000) { first.isClosed }
        #expect(client.isConnected, "the client does not know yet")
        try await eventually("the server is free", tries: 3_000) { await !server.isServing }
        let writtenBefore = first.written.withLock { $0.count }
        let tap = RunnerCommand.tap(x: 3, y: 4)
        #expect(try await client.perform(tap).ok)
        #expect(fake.state.withLock { $0.commands } == [tap], "carried out exactly once")
        #expect(connector.ends.withLock { $0.count } == 2)
        // Nothing of the tap went into the closed connection.
        #expect(first.written.withLock { $0.count } == writtenBefore)
    }

    @Test("retry: a command whose request was written completely is never sent again")
    func performAfterWriteNotResent() async throws {
        let rogue = DroppingRunner()
        let connector = RawConnector { rogue.serve($0) }
        let client = makeClient(connector)
        do {
            _ = try await client.perform(.tap(x: 1, y: 1))
            Issue.record("no error")
        } catch let error as JarvisRunnerError {
            guard case .unreachable = error else { Issue.record("\(error)"); return }
        }
        #expect(connector.count == 1, "the tap was sent again")
        #expect(rogue.requests.withLock { $0 } == ["tap"])
    }

    @Test("retry: a command whose request was cut off mid-write is never sent again")
    func performAfterPartialWriteNotResent() async throws {
        let fake = TypedFakeRunner()
        let server = makeServer(fake)
        // Writes: hello, finish, hello request, then the tap: cut in half.
        let connector = RawConnector(failWrite: 4) { far in Task { await server.accept(far) } }
        let client = makeClient(connector)
        await #expect(throws: JarvisRunnerError.self) { try await client.perform(.tap(x: 1, y: 1)) }
        #expect(connector.count == 1, "the tap was sent again")
        #expect(fake.state.withLock { $0.commands }.isEmpty)
    }

    @Test("retry: a read whose connection fails after the write is asked once more; a second failure surfaces")
    func readRetriedOnce() async throws {
        // Fails once, then a real runner answers.
        let rogue = DroppingRunner()
        let fake = TypedFakeRunner()
        let server = makeServer(fake)
        let connector = RawConnector { far in
            if rogue.connections.withLock({ $0 }) == 0 { rogue.serve(far) } else { Task { await server.accept(far) } }
        }
        let client = makeClient(connector)
        #expect(try await client.lockState() == .unlocked)
        #expect(connector.count == 2)
        #expect(rogue.requests.withLock { $0 } == ["lock state"])

        // Fails every time: two attempts, then the error.
        let always = DroppingRunner()
        let failing = RawConnector { always.serve($0) }
        let reader = makeClient(failing)
        do {
            _ = try await reader.snapshot(includeScreenshot: false)
            Issue.record("no error")
        } catch let error as JarvisRunnerError {
            guard case .unreachable = error else { Issue.record("\(error)"); return }
        }
        #expect(failing.count == 2, "a read is retried exactly once")
        #expect(always.requests.withLock { $0 } == ["snapshot", "snapshot"])
    }

    @Test("LOW-3: a request the runner receives after its not_after is not carried out")
    func heldRequestRefused() async throws {
        let fake = TypedFakeRunner()
        let clientClock = RunnerTestClock()
        let serverClock = RunnerTestClock()
        let server = makeServer(fake, clock: serverClock.function)
        let client = makeClient(PipeConnector(server), clock: clientClock.function)
        #expect(try await client.lockState() == .unlocked)
        // The runner's reading of the shared clock is past the command's deadline when the
        // request arrives (as if it had been held back that long).
        serverClock.advance(JarvisRunnerTiming.clientCommand.wholeMilliseconds + 1)
        await #expect(throws: JarvisRunnerError.self) { try await client.perform(.tap(x: 1, y: 2)) }
        #expect(fake.state.withLock { $0.commands }.isEmpty, "an expired tap was carried out")
        #expect(!client.isConnected)
    }

    @Test("LOW-3: the shared clock moves forward")
    func uptimeClock() async throws {
        let a = RunnerUptimeClock.nowMs()
        try await Task.sleep(for: .milliseconds(20))
        let b = RunnerUptimeClock.nowMs()
        #expect(a > 0 && b >= a + 10)
    }
}
