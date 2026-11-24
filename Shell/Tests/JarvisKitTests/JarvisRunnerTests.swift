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

    init(label: String = "Playground", children: Int = 3) {
        self.label = label
        root = TypedFakeRunner.tree(label: label, children: children)
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
        state.withLock { s in
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
    events log: EventLog? = nil
) -> RunnerServer {
    RunnerServer(runner: runner, key: Redacted(serverKey), random: GeneratorRandomSource(),
                 configuration: configuration, onEvent: { e in log?.events.withLock { $0.append(e) } })
}

private final class EventLog: Sendable {
    let events = Mutex<[RunnerServer.Event]>([])
}

private func makeClient(
    _ connector: any RunnerConnector,
    key clientKey: Data? = key,
    configuration: JarvisRunnerClient.Configuration = JarvisRunnerClient.Configuration()
) -> JarvisRunnerClient {
    let source: any RunnerKeySource = clientKey.map { FixedRunnerKeySource($0) } ?? NoKey()
    return JarvisRunnerClient(connector: connector, keySource: source, random: GeneratorRandomSource(), configuration: configuration)
}

private struct NoKey: RunnerKeySource {
    func runnerKey() throws -> Redacted<Data>? { nil }
}

/// Waits (bounded) until `condition` holds.
private func eventually(_ what: String, _ condition: @Sendable () async -> Bool) async throws {
    for _ in 0..<200 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    Issue.record("timed out waiting for: \(what)")
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
            if let events = try? server.receive(bytes: bytes) {
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
}
