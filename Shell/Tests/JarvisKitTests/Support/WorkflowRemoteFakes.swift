import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

func scratchDirectory(_ name: String) -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-\(name)-\(UUID().uuidString)")
}

/// The owner, answering prompts by rule (as the sheets' buttons would). Confirmations whose
/// category is in `approve` are approved, those in `decline` declined, others left on
/// screen; yes/no questions get `yes`.
final class ScriptedOwner: Sendable {
    struct Rules {
        var approve: Set<RiskCategory> = [.firstUse]
        var decline: Set<RiskCategory> = []
        /// `nil`: leave yes/no questions unanswered.
        var yes: Bool? = true
    }

    let rules: Mutex<Rules>
    let seen = Mutex<[HumanPrompt]>([])
    private let ref = Mutex<HumanPromptBroker?>(nil)

    init(_ rules: Rules = Rules()) {
        self.rules = Mutex(rules)
    }

    var broker: HumanPromptBroker {
        if let b = ref.withLock({ $0 }) { return b }
        let broker = HumanPromptBroker { [weak self] prompt in
            guard let self, let prompt else { return }
            self.seen.withLock { $0.append(prompt) }
            let rules = self.rules.withLock { $0 }
            Task { [weak self] in
                guard let broker = self?.ref.withLock({ $0 }) else { return }
                switch prompt {
                case .confirm(let id, let request):
                    if rules.approve.contains(request.category) { broker.approve(id) }
                    if rules.decline.contains(request.category) { broker.decline(id) }
                case .yesNo(let id, _):
                    if let yes = rules.yes { broker.answerYesNo(id, yes: yes) }
                case .question:
                    break
                }
            }
        }
        ref.withLock { $0 = broker }
        return broker
    }

    var confirmCategories: [RiskCategory] {
        seen.withLock { $0.compactMap { if case .confirm(_, let r) = $0 { r.category } else { nil } } }
    }
}

/// Records notify steps.
final class RecordingNotifier: WorkflowNotifier {
    let posted = Mutex<[String]>([])
    func post(_ text: String, workflow: String) async throws {
        posted.withLock { $0.append(text) }
    }
}

/// Everything a workflow or remote test needs, on the demo backend (DemoPhone + scripted
/// model) and the real core.
@MainActor
final class AppHarness {
    let dir: URL
    let owner: ScriptedOwner
    let coordinator: JarvisCoordinator
    let center: TaskCenter
    let skills: SkillLibrary
    let library: WorkflowLibrary
    let notifier = RecordingNotifier()
    let runner: WorkflowRunner

    init(_ name: String, rules: ScriptedOwner.Rules = ScriptedOwner.Rules(), limits: RunLimits? = nil,
         libraryNow: @escaping @Sendable () -> Date = Date.init,
         clock: any MonotonicClock = SystemMonotonicClock(),
         sleep: (@Sendable (Duration) async throws -> Void)? = nil,
         settingsProblem: @escaping @MainActor () -> String? = { nil },
         demoRunner: (@Sendable (Duration, Int) -> any PhoneRunner)? = nil) {
        dir = scratchDirectory(name)
        owner = ScriptedOwner(rules)
        let paths = JarvisPaths(directory: dir)
        coordinator = JarvisCoordinator(
            paths: paths, selection: .demo, agentBundleIds: JarvisOwnBundles.ids(main: "dev.jarvis.app"), tools: nil, configuration: DriverConfiguration(),
            demoDelay: .zero, demoLockedPolls: 0, maxLockedWaitOverrideMs: nil, viewerFrames: ViewerFrameStore(),
            approvalFrames: ApprovalFrameStore(), demoRunner: demoRunner
        )
        center = TaskCenter(coordinator: coordinator, broker: owner.broker, settings: { JarvisSettings() },
                            settingsProblem: settingsProblem)
        skills = SkillLibrary(fileURL: paths.skills)
        library = WorkflowLibrary(fileURL: paths.workflows, skills: skills, now: libraryNow)
        runner = WorkflowRunner(library: library, center: center, notifier: notifier, clock: clock, limits: limits, sleep: sleep)
    }

    /// Stops whatever a test left running (a failed test must not leave a session waiting
    /// in the background for the rest of the run), then removes the files.
    func cleanUp() {
        runner.cancel()
        center.cancelSession()
        try? FileManager.default.removeItem(at: dir)
    }

    var history: [TaskRecord] { TaskHistoryStore(fileURL: coordinator.paths.history).load() }
}

/// Polls on the main actor (yielding it between checks) until `condition` holds.
@MainActor
func eventually(_ what: String, seconds: Double = 10, _ condition: @MainActor () -> Bool) async throws {
    let deadline = ContinuousClock.now + .milliseconds(Int(seconds * 1_000))
    while ContinuousClock.now < deadline {
        if condition() { return }
        try await Task.sleep(for: .milliseconds(5))
    }
    Issue.record("timed out waiting for \(what)")
    throw WaitTimeout()
}

// MARK: - Remote fakes

/// One ordered log shared by the fake store and the recording transport.
final class IOLog: Sendable {
    enum Entry: Equatable, Sendable {
        case write
        case send(UInt64, Data)
        case close(UInt64)
    }

    let entries = Mutex<[Entry]>([])
    func add(_ e: Entry) { entries.withLock { $0.append(e) } }
    var all: [Entry] { entries.withLock { $0 } }
}

final class RecordingSecretStore: SecretStore {
    let log: IOLog
    let inner = InMemorySecretStore()
    let failWrites = Mutex(false)
    let writes = Mutex(0)

    init(log: IOLog) { self.log = log }

    func read(_ key: String) throws -> String? { try inner.read(key) }

    func write(_ key: String, _ value: String) throws {
        if failWrites.withLock({ $0 }) { throw InjectedError(message: "the keychain is unavailable") }
        log.add(.write)
        writes.withLock { $0 += 1 }
        try inner.write(key, value)
    }

    func delete(_ key: String) throws { try inner.delete(key) }
}

final class RecordingTransport: RemoteTransport {
    let inner: InMemoryRemoteTransport
    let log: IOLog

    init(_ inner: InMemoryRemoteTransport, log: IOLog) {
        self.inner = inner
        self.log = log
    }

    func start() -> AsyncStream<RemoteTransportEvent> { inner.start() }

    func send(_ bytes: Data, to connection: UInt64) {
        log.add(.send(connection, bytes))
        inner.send(bytes, to: connection)
    }

    func send(_ bytes: Data, to connection: UInt64, written: @escaping @Sendable (Int) -> Void) {
        log.add(.send(connection, bytes))
        inner.send(bytes, to: connection, written: written)
    }

    func consumed(_ count: Int, on connection: UInt64) { inner.consumed(count, on: connection) }

    func close(_ connection: UInt64) {
        log.add(.close(connection))
        inner.close(connection)
    }

    func stop() { inner.stop() }

    func localAddresses() -> [RemotePhoneAddress] { inner.localAddresses() }
}

/// Bytes that are obviously not random (the hub must refuse them).
struct ZeroRandomSource: RandomSource {
    func bytes(count: Int) throws -> Data { Data(count: count) }
}

/// The real hub with hooks: extra effects on the next `tick`, and a poisoned call.
final class HubProbe: JarvisRemoteHubProtocol, @unchecked Sendable {
    let hub: JarvisRemoteHub
    let injectOnTick = Mutex<[HubEffect]?>(nil)
    let poisonNext = Mutex(false)
    /// Every effect list the hub returned, in order.
    let returned = Mutex<[[HubEffect]]>([])

    /// A confirmation the phone reported to the hub, with the request id the hub gave it.
    struct ConfirmAnnounced: Sendable {
        let category: RiskCategory
        let requestId: UInt64?
    }

    /// Every `confirmRequested` the hub was told about, in order. The hub's
    /// `pendingRequestId()` alone does not say WHICH confirmation is pending: right after the
    /// owner answered one prompt, the phone can already show the next one while the hub
    /// still holds the previous request (the run's events reach remote control through a
    /// stream, a moment later).
    let confirmations = Mutex<[ConfirmAnnounced]>([])

    /// The hub's request id for the latest confirmation of `category` the phone reported.
    func requestId(for category: RiskCategory) -> UInt64? {
        confirmations.withLock { $0.last { $0.category == category }?.requestId }
    }

    private func keep(_ effects: [HubEffect]) -> [HubEffect] {
        returned.withLock { $0.append(effects) }
        return effects
    }

    init(_ hub: JarvisRemoteHub) { self.hub = hub }

    private func check() throws {
        if poisonNext.withLock({ p in defer { p = false }; return p }) {
            throw JarvisError.Poisoned(message: "injected")
        }
    }

    func cancelPairing() throws { try check(); try hub.cancelPairing() }
    func clockMs() throws -> UInt64 { try hub.clockMs() }
    func connectionClosed(connId: UInt64, nowMs: UInt64) throws -> [HubEffect] { try check(); return keep(try hub.connectionClosed(connId: connId, nowMs: nowMs)) }
    func connectionCount() throws -> UInt32 { try hub.connectionCount() }
    func connectionOpened(connId: UInt64, nowMs: UInt64, entropy: Data) throws -> [HubEffect] {
        try check(); return keep(try hub.connectionOpened(connId: connId, nowMs: nowMs, entropy: entropy))
    }
    func deviceLockedUntilMs(deviceId: String) throws -> UInt64 { try hub.deviceLockedUntilMs(deviceId: deviceId) }
    func devices() throws -> [DeviceInfo] { try hub.devices() }
    func isAuthenticated(connId: UInt64) throws -> Bool { try hub.isAuthenticated(connId: connId) }
    func isBusy() throws -> Bool { try hub.isBusy() }
    func isPairingOpen() throws -> Bool { try hub.isPairingOpen() }
    func localEvent(nowMs: UInt64, event: LocalEvent) throws -> [HubEffect] {
        try check()
        let effects = keep(try hub.localEvent(nowMs: nowMs, event: event))
        if case .confirmRequested(let request) = event {
            let id = try hub.pendingRequestId()
            confirmations.withLock { $0.append(ConfirmAnnounced(category: request.category, requestId: id)) }
        }
        return effects
    }
    func openPairing(nowMs: UInt64, entropy: Data) throws -> PairingOffer { try check(); return try hub.openPairing(nowMs: nowMs, entropy: entropy) }
    func pairingLockedUntilMs() throws -> UInt64 { try hub.pairingLockedUntilMs() }
    func pendingRequestId() throws -> UInt64? { try hub.pendingRequestId() }
    func receive(connId: UInt64, bytes: Data, nowMs: UInt64) throws -> [HubEffect] { try check(); return keep(try hub.receive(connId: connId, bytes: bytes, nowMs: nowMs)) }
    func renameDevice(nowMs: UInt64, deviceId: String, name: String) throws -> [HubEffect] { try check(); return try hub.renameDevice(nowMs: nowMs, deviceId: deviceId, name: name) }
    func revokeDevice(nowMs: UInt64, deviceId: String) throws -> [HubEffect] { try check(); return keep(try hub.revokeDevice(nowMs: nowMs, deviceId: deviceId)) }
    func setPermissions(nowMs: UInt64, deviceId: String, permissions: RemotePermissions) throws -> [HubEffect] {
        try check(); return try hub.setPermissions(nowMs: nowMs, deviceId: deviceId, permissions: permissions)
    }
    func tick(nowMs: UInt64) throws -> [HubEffect] {
        try check()
        let extra = injectOnTick.withLock { e in defer { e = nil }; return e }
        return keep((extra ?? []) + (try hub.tick(nowMs: nowMs)))
    }
    func toJson() throws -> String { try hub.toJson() }
    func written(connId: UInt64, bytes: UInt64, nowMs: UInt64) throws -> [HubEffect] {
        try check(); return keep(try hub.written(connId: connId, bytes: bytes, nowMs: nowMs))
    }
    func setWatchScreen(nowMs: UInt64, deviceId: String, allowed: Bool) throws -> [HubEffect] {
        try check(); return try hub.setWatchScreen(nowMs: nowMs, deviceId: deviceId, allowed: allowed)
    }
    func watchScreen(deviceId: String) throws -> Bool { try hub.watchScreen(deviceId: deviceId) }
    func pendingDevices() throws -> [PendingDevice] { try hub.pendingDevices() }
    func confirmDevice(nowMs: UInt64, deviceId: String) throws -> [HubEffect] {
        try check(); return keep(try hub.confirmDevice(nowMs: nowMs, deviceId: deviceId))
    }
    func pairingFailedAttempts() throws -> UInt32 { try hub.pairingFailedAttempts() }
    func isWatching(connId: UInt64) throws -> Bool { try hub.isWatching(connId: connId) }
}

/// The probe streams the Live view through the real hub it wraps.
extension HubProbe: LiveViewStreamingHub {
    var liveViewHub: JarvisRemoteHub { hub }
}

/// A remote-control setup on the demo backend.
@MainActor
final class RemoteHarness {
    let base: AppHarness
    let log = IOLog()
    let transport = InMemoryRemoteTransport()
    let secrets: RecordingSecretStore
    final class Probes: Sendable { let list = Mutex<[HubProbe]>([]) }
    let probes = Probes()
    let service: RemoteControlService
    /// The scripted owner confirms every newly paired device on the phone, as the owner
    /// does after comparing the check strings (the confirm step's own tests turn it off and
    /// confirm or remove by hand).
    var autoConfirm = true
    /// Devices the scripted owner confirmed.
    private(set) var confirmedByOwner: [String] = []
    private var confirming = false

    init(_ name: String, rules: ScriptedOwner.Rules = ScriptedOwner.Rules(), random: any RandomSource = GeneratorRandomSource(),
         settingsProblem: @escaping @MainActor () -> String? = { nil },
         demoRunner: (@Sendable (Duration, Int) -> any PhoneRunner)? = nil,
         clock: (@Sendable () -> UInt64)? = nil) {
        base = AppHarness(name, rules: rules, settingsProblem: settingsProblem, demoRunner: demoRunner)
        secrets = RecordingSecretStore(log: log)
        let transport = RecordingTransport(self.transport, log: log)
        let probes = self.probes
        service = RemoteControlService(
            secrets: secrets, random: random, transport: { transport }, phoneName: "Test phone",
            center: base.center, runner: base.runner,
            clock: clock ?? { UInt64(max(0, Date().timeIntervalSince1970 * 1_000)) }, tickInterval: .seconds(3_600),
            makeHub: { json, name in
                let probe = HubProbe(try JarvisRemoteHub(storeJson: json, phoneName: name))
                probes.list.withLock { $0.append(probe) }
                return probe
            }
        )
        service.onChange = { [weak self] in self?.confirmPending() }
    }

    /// The scripted owner's Confirm on the phone (only while `autoConfirm`).
    private func confirmPending() {
        guard autoConfirm, !confirming else { return }
        confirming = true
        defer { confirming = false }
        for pending in service.pendingDevices where !confirmedByOwner.contains(pending.device.id) {
            confirmedByOwner.append(pending.device.id)
            try? service.confirm(deviceId: pending.device.id)
        }
    }

    var probe: HubProbe? { probes.list.withLock { $0.last } }
    var hubsMade: Int { probes.list.withLock { $0.count } }

    /// Turns remote control on, pairs a device named `name` and returns it connected.
    func pairDevice(_ name: String = "Test Mac") async throws -> InProcessRemote {
        if !service.isOn { service.setEnabled(true) }
        try await eventually("the listener") { transport.isListening }
        let offer = try service.openPairing()
        let before = service.devices.count
        let remote = try await InProcessRemote.pair(code: offer.code, name: name, over: transport)
        // The hub keeps a device once its first authenticated frame arrives, which can be
        // just after the client says it is connected.
        try await eventually("the device to be kept") { service.devices.count > before }
        return remote
    }

    func cleanUp() {
        service.setEnabled(false)
        base.cleanUp()
    }
}
