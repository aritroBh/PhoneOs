import Foundation
import JarvisCore

/// What can go wrong with remote control, in the owner's words.
public enum RemoteControlError: Error, Sendable, Equatable, CustomStringConvertible {
    case off
    /// The system's random source failed or gave bytes the core refused. Nothing falls back
    /// to another source.
    case randomSource(String)
    /// Pairing is locked after failed attempts until this hub time (ms).
    case pairingLocked(untilMs: UInt64)
    case limitReached(String)
    case unknownDevice
    case invalidName
    /// The saved state could not be read or the core refused it (it is kept, not replaced).
    case store(String)
    case core(String)

    public var description: String {
        switch self {
        case .off: "Remote control is off."
        case .randomSource(let why): "The phone's random source failed (\(why)). Nothing was opened."
        case .pairingLocked: "Pairing is paused after failed attempts. Try again later."
        case .limitReached(let why): "Limit reached: \(why)."
        case .unknownDevice: "That device is no longer paired."
        case .invalidName: "The name is empty."
        case .store(let why): "Remote control's saved state could not be used: \(why)"
        case .core(let why): why
        }
    }

    init(core error: any Error) {
        guard let e = error as? JarvisError else {
            self = .core(String(describing: error))
            return
        }
        switch e {
        case .RemoteEntropy: self = .randomSource(CoreErrorMessage(e).message)
        case .RemotePairingLocked(let until): self = .pairingLocked(untilMs: until)
        case .RemoteLimitReached: self = .limitReached(CoreErrorMessage(e).message)
        case .RemoteUnknownDevice: self = .unknownDevice
        case .RemoteInvalidName: self = .invalidName
        case .RemoteStoreInvalid(let m): self = .store(m)
        default: self = .core(CoreErrorMessage(e).message)
        }
    }
}

/// A pairing offer on screen. The code and the QR payload are SECRET: they are wrapped so
/// printing the offer shows nothing, and the service drops the offer when it is used,
/// cancelled or expired.
public struct RemotePairingOffer: Sendable, Equatable, CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
    public let code: Redacted<String>
    public let qrPayload: Redacted<String>
    /// When the offer stops being accepted (the hub's clock, ms).
    public let expiresAtMs: UInt64

    public var description: String { "RemotePairingOffer(<redacted>, expires \(expiresAtMs))" }
    public var debugDescription: String { description }
    public var customMirror: Mirror { Mirror(self, children: ["expiresAtMs": expiresAtMs], displayStyle: .struct) }
}

/// Remote control on the phone: the core's `JarvisRemoteHub` plus the IO it needs.
///
/// - **Effects in order, persistence first.** Every hub call goes through one FIFO inbox;
///   its `[HubEffect]` are carried out in order, and `storeChanged` writes `toJson()` to the
///   `SecretStore` before the next effect runs. Events the effects cause on the phone (a
///   session starting) are queued behind them, so frames always leave in the hub's order.
///   If the write fails, nothing after it goes out and every connection is closed.
/// - **One entry for tasks.** Remote `startTask`/`runSkill`/`runWorkflow` start through
///   `TaskCenter` / `WorkflowRunner` exactly like the owner's requests; skills are checked
///   again with `SkillLibrary.context`.
/// - **Decisions are bound to a prompt id.** When the hub gets a request for a confirmation,
///   the broker's prompt id of that confirmation is recorded with the hub's request id (the
///   first confirmation prompt with that request not bound before; the next one shown when it
///   is not on screen yet). A decision resolves only that exact prompt, and only while it is
///   still the pending one; anything else (a second device after the first one decided, an
///   answer the owner already gave on the phone, a prompt replaced by an equal one after a
///   lock or in step-by-step mode) is refused and written to the audit log as stale. The
///   category and the request on screen are checked too, and an approval also needs
///   `remotelyApprovable(category:)`.
/// - **Lifecycle.** Off by default; on starts the listener and a tick timer; off stops both
///   and drops the hub. A poisoned hub is rebuilt from the secret store.
@MainActor
public final class RemoteControlService {
    public enum Status: Sendable, Equatable {
        case off
        case starting
        case listening(String?)
        case failed(String)
        /// The saved state was refused or unreadable; it is kept, remote control stays off.
        case storeProblem(String)
    }

    public static let secretKey = "remote-hub"

    public private(set) var status: Status = .off
    public private(set) var devices: [DeviceInfo] = []
    public private(set) var pairing: RemotePairingOffer?
    /// The last problem worth showing (refused entropy, a failed write, a rebuilt hub).
    public private(set) var lastError: String?
    /// Connections that finished their handshake, by connection id → device id.
    public private(set) var connected: [UInt64: String] = [:]
    public let audit: RemoteAuditLog
    /// Called after every change.
    public var onChange: (@MainActor () -> Void)?

    private let secrets: any SecretStore
    private let random: any RandomSource
    private let makeTransport: @MainActor () -> any RemoteTransport
    private let makeHub: @Sendable (String, String) throws -> any JarvisRemoteHubProtocol
    private let phoneName: String
    private let center: TaskCenter
    private let runner: WorkflowRunner
    private let desk: any ConfirmationDesk
    private let clock: @Sendable () -> UInt64
    private let tickInterval: Duration

    private var hub: (any JarvisRemoteHubProtocol)?
    private var transport: (any RemoteTransport)?
    private var consumer: Task<Void, Never>?
    private var ticker: Task<Void, Never>?
    private var open: Set<UInt64> = []
    private var observer: UUID?

    private enum Input {
        case hub(@MainActor (any JarvisRemoteHubProtocol) throws -> [HubEffect])
        case confirmRequested(ConfirmRequest)
        case confirmResolved(approved: Bool)
        case local(LocalEvent)
    }

    private var inbox: [Input] = []
    private var working = false
    /// The confirmation the hub knows about: its request id, the request, and the broker's
    /// prompt id it was announced for (`nil` until that prompt is shown).
    private var hubConfirm: (id: UInt64, request: ConfirmRequest, promptId: Int?)?
    /// The highest broker prompt id already bound to a hub request (or shown before remote
    /// control started): a later request binds only to a newer prompt.
    private var boundPromptId = 0
    private var lastScreen: Screen?
    private var lastCatalog: (skills: [CatalogEntry], workflows: [CatalogEntry])?

    public init(
        secrets: any SecretStore,
        random: any RandomSource,
        transport makeTransport: @escaping @MainActor () -> any RemoteTransport,
        phoneName: String,
        center: TaskCenter,
        runner: WorkflowRunner,
        desk: (any ConfirmationDesk)? = nil,
        audit: RemoteAuditLog = RemoteAuditLog(),
        clock: @escaping @Sendable () -> UInt64 = { UInt64(max(0, Date().timeIntervalSince1970 * 1_000)) },
        tickInterval: Duration = .seconds(1),
        makeHub: @escaping @Sendable (String, String) throws -> any JarvisRemoteHubProtocol = { try JarvisRemoteHub(storeJson: $0, phoneName: $1) }
    ) {
        self.secrets = secrets
        self.random = random
        self.makeTransport = makeTransport
        self.phoneName = phoneName
        self.center = center
        self.runner = runner
        self.desk = desk ?? center.broker
        self.audit = audit
        self.clock = clock
        self.tickInterval = tickInterval
        self.makeHub = makeHub
    }

    public var isOn: Bool { hub != nil }

    /// The hub's clock now (what `expiresAtMs` and lockouts are measured on).
    public func nowMs() -> UInt64 { clock() }

    // MARK: - On / off

    /// Turns remote control on (loads the hub from the secret store, starts the listener
    /// and the tick timer) or off (closes every connection, stops the listener).
    public func setEnabled(_ on: Bool) {
        on ? start() : stop()
        changed()
    }

    private func start() {
        guard hub == nil else { return }
        guard loadHub() else { return }
        startTransport()
        let interval = tickInterval
        ticker = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: interval)
                if Task.isCancelled { return }
                self?.tick()
            }
        }
        if observer == nil {
            observer = center.observe { [weak self] event in self?.taskEvent(event) }
        }
        // Prompts shown before remote control started were never announced to the hub.
        boundPromptId = max(boundPromptId, desk.latestPromptId)
        lastCatalog = nil
        refreshCatalog()
        if let session = center.session, session.origin.workflow == nil {
            enqueue(.local(.sessionStarted(taskId: session.origin.remoteTaskId)))
        } else if let workflow = center.workflow {
            enqueue(.local(.sessionStarted(taskId: workflow.origin.remoteTaskId)))
        }
    }

    /// Makes and starts a transport and the task that consumes its events. Every `.received`
    /// is acknowledged to the transport that produced it (`consumed`) once it was handled.
    private func startTransport() {
        let transport = makeTransport()
        self.transport = transport
        status = .starting
        let events = transport.start()
        consumer = Task { @MainActor [weak self] in
            for await event in events {
                self?.handle(event)
                if case .received(let id, let bytes) = event { transport.consumed(bytes.count, on: id) }
            }
        }
    }

    /// Stops the transport only (the hub stays): its connections are closed and the hub is
    /// told so.
    private func stopTransport() {
        consumer?.cancel()
        consumer = nil
        transport?.stop()
        transport = nil
        let closed = open.sorted()
        open = []
        for id in closed {
            connected[id] = nil
            enqueue(.hub { [weak self] hub in try hub.connectionClosed(connId: id, nowMs: self?.clock() ?? 0) })
        }
    }

    /// Restarts the transport when remote control is on but its listener failed (an
    /// `NWListener` commonly fails while the app is suspended). The app calls it whenever it
    /// becomes active. Returns whether it restarted the transport.
    @discardableResult
    public func ensureListening() -> Bool {
        guard hub != nil, case .failed = status else { return false }
        stopTransport()
        startTransport()
        changed()
        return true
    }

    /// Loads the hub; false (and `.storeProblem`) when the saved state cannot be used. The
    /// saved state is never overwritten here.
    private func loadHub() -> Bool {
        let json: String
        do {
            json = try secrets.read(Self.secretKey) ?? ""
        } catch {
            status = .storeProblem("the saved state could not be read (\(error))")
            return false
        }
        do {
            hub = try makeHub(json, phoneName)
        } catch {
            status = .storeProblem(RemoteControlError(core: error).description)
            return false
        }
        refreshDevices()
        return true
    }

    private func stop() {
        guard let hub else {
            if case .storeProblem = status {} else { status = .off }
            return
        }
        for id in open.sorted() {
            if let effects = try? hub.connectionClosed(connId: id, nowMs: clock()) { carryOut(effects, closing: true) }
        }
        consumer?.cancel()
        ticker?.cancel()
        consumer = nil
        ticker = nil
        transport?.stop()
        transport = nil
        open = []
        connected = [:]
        pairing = nil
        hubConfirm = nil
        inbox = []
        self.hub = nil
        status = .off
    }

    // MARK: - Owner actions

    /// Opens a single-use pairing offer (5 minutes). Throws `.randomSource` when the
    /// system's random bytes fail or are refused, `.pairingLocked`, `.limitReached`.
    @discardableResult
    public func openPairing() throws -> RemotePairingOffer {
        guard let hub else { throw RemoteControlError.off }
        let entropy: Data
        do {
            entropy = try random.bytes(count: Int(remoteMinEntropyLen()))
        } catch {
            let failure = RemoteControlError.randomSource(String(describing: error))
            lastError = failure.description
            changed()
            throw failure
        }
        do {
            let offer = try hub.openPairing(nowMs: clock(), entropy: entropy)
            let wrapped = RemotePairingOffer(code: Redacted(offer.code), qrPayload: Redacted(offer.qrPayload),
                                             expiresAtMs: offer.expiresAtMs)
            pairing = wrapped
            changed()
            return wrapped
        } catch {
            let failure = handleHubError(error)
            if case .randomSource = failure { lastError = failure.description }
            changed()
            throw failure
        }
    }

    /// Withdraws the pairing offer (the owner closed the pairing screen).
    public func cancelPairing() {
        pairing = nil
        try? hub?.cancelPairing()
        changed()
    }

    public var pairingLockedUntilMs: UInt64 { (try? hub?.pairingLockedUntilMs()) ?? 0 }

    public func deviceLockedUntilMs(_ id: String) -> UInt64 { (try? hub?.deviceLockedUntilMs(deviceId: id)) ?? 0 }

    public func rename(deviceId: String, to name: String) throws {
        try ownerAction { try $0.renameDevice(nowMs: self.clock(), deviceId: deviceId, name: name) }
    }

    public func setPermissions(deviceId: String, _ permissions: RemotePermissions) throws {
        try ownerAction { try $0.setPermissions(nowMs: self.clock(), deviceId: deviceId, permissions: permissions) }
    }

    /// Removes a device at once: its key is forgotten, its connections close and the task it
    /// asked for stops (never one the owner started).
    public func revoke(deviceId: String) throws {
        try ownerAction { try $0.revokeDevice(nowMs: self.clock(), deviceId: deviceId) }
    }

    public func isConnected(_ deviceId: String) -> Bool { connected.values.contains(deviceId) }

    public func deviceName(_ id: String) -> String? { devices.first { $0.id == id }?.name }

    private func ownerAction(_ call: @escaping @MainActor (any JarvisRemoteHubProtocol) throws -> [HubEffect]) throws {
        guard let hub else { throw RemoteControlError.off }
        let effects: [HubEffect]
        do {
            effects = try call(hub)
        } catch {
            let failure = handleHubError(error)
            changed()
            throw failure
        }
        enqueue(.hub { _ in effects })
    }

    /// Tells remotes what they may list and run: the enabled skills and workflows. Sent
    /// only when it changed.
    public func refreshCatalog() {
        guard hub != nil else { return }
        let skills = runner.library.skills.catalog
        let workflows = runner.library.catalog
        if let last = lastCatalog, last.skills == skills, last.workflows == workflows { return }
        lastCatalog = (skills, workflows)
        enqueue(.local(.catalogChanged(skills: skills, workflows: workflows)))
    }

    // MARK: - Transport

    private func handle(_ event: RemoteTransportEvent) {
        guard hub != nil else { return }
        switch event {
        case .listening(let detail):
            status = .listening(detail)
        case .failed(let why):
            status = .failed(why)
        case .opened(let id):
            let entropy: Data
            do {
                entropy = try random.bytes(count: Int(remoteMinEntropyLen()))
            } catch {
                lastError = RemoteControlError.randomSource(String(describing: error)).description
                transport?.close(id)
                break
            }
            enqueue(.hub { [weak self] hub in
                do {
                    let effects = try hub.connectionOpened(connId: id, nowMs: self?.clock() ?? 0, entropy: entropy)
                    self?.open.insert(id)
                    return effects
                } catch let error as JarvisError {
                    // The hub did not register the connection: close it here.
                    if case .RemoteEntropy = error {
                        self?.transport?.close(id)
                        self?.lastError = RemoteControlError(core: error).description
                        return []
                    }
                    throw error
                }
            })
        case .received(let id, let bytes):
            guard open.contains(id) else { break }
            enqueue(.hub { [weak self] hub in try hub.receive(connId: id, bytes: bytes, nowMs: self?.clock() ?? 0) })
        case .closed(let id):
            guard open.remove(id) != nil else { break }
            connected[id] = nil
            enqueue(.hub { [weak self] hub in try hub.connectionClosed(connId: id, nowMs: self?.clock() ?? 0) })
        }
        changed()
    }

    /// Applies timeouts and expiry (called by the timer; tests call it directly).
    public func tick() {
        guard hub != nil else { return }
        enqueue(.hub { [weak self] hub in try hub.tick(nowMs: self?.clock() ?? 0) })
        if pairing != nil, (try? hub?.isPairingOpen()) != true {
            pairing = nil
        }
        changed()
    }

    // MARK: - The inbox

    private func enqueue(_ input: Input) {
        inbox.append(input)
        guard !working else { return }
        working = true
        while !inbox.isEmpty {
            let next = inbox.removeFirst()
            guard let hub else { inbox = []; break }
            do {
                switch next {
                case .hub(let call):
                    carryOut(try call(hub))
                case .local(let event):
                    carryOut(try hub.localEvent(nowMs: clock(), event: event))
                case .confirmRequested(let request):
                    let effects = try hub.localEvent(nowMs: clock(), event: .confirmRequested(request: request))
                    if let id = try hub.pendingRequestId() { hubConfirm = (id, request, bindPrompt(for: request)) }
                    carryOut(effects)
                case .confirmResolved(let approved):
                    guard let pending = hubConfirm else { break }
                    hubConfirm = nil
                    carryOut(try hub.localEvent(nowMs: clock(), event: .confirmResolved(requestId: pending.id, approved: approved)))
                }
            } catch {
                _ = handleHubError(error)
            }
        }
        working = false
        refreshDevices()
        changed()
    }

    /// Carries out `effects` in order. `storeChanged` is written before anything after it;
    /// if that write fails, only `close` effects still run and every connection is closed.
    private func carryOut(_ effects: [HubEffect], closing: Bool = false) {
        var persistFailed = false
        for effect in effects {
            if persistFailed {
                if case .close(let id, _) = effect { closeConnection(id) }
                continue
            }
            switch effect {
            case .storeChanged:
                do {
                    guard let hub else { continue }
                    try secrets.write(Self.secretKey, try hub.toJson())
                } catch {
                    persistFailed = true
                    lastError = "Remote control could not save its state, so every connection was closed."
                    for id in open.sorted() { closeConnection(id) }
                }
            case .send(let id, let bytes):
                transport?.send(bytes, to: id)
            case .close(let id, _):
                closeConnection(id)
            case .startTask(let taskId, let deviceId, let text):
                guard !closing else { continue }
                startRemote(taskId: taskId, deviceId: deviceId, text: text, skill: nil)
            case .runSkill(let taskId, let deviceId, let id, let args):
                guard !closing else { continue }
                // The hub only checked that the id is in the catalog; the values and the
                // skill as it is now are checked here, by the core.
                do {
                    let skills = runner.library.skills
                    let context = try skills.context(skillId: id, values: args)
                    let text = try skills.taskText(skillId: id, values: args)
                    startRemote(taskId: taskId, deviceId: deviceId, text: text, skill: context)
                } catch {
                    note("Refused a skill from \(deviceName(deviceId) ?? "a device"): \(error)", device: deviceId, warning: true)
                    enqueue(.local(.sessionFinished(taskId: taskId, outcome: Self.failed("The skill could not run: \(error)"))))
                }
            case .runWorkflow(let taskId, let deviceId, let id):
                guard !closing else { continue }
                let origin = TaskOrigin.remote(taskId: taskId, deviceId: deviceId, deviceName: deviceName(deviceId))
                if case .failure(let refusal) = runner.start(id: id, origin: origin) {
                    enqueue(.local(.sessionFinished(taskId: taskId, outcome: Self.failed(refusal.description))))
                }
            case .cancelTask(_, let taskId):
                cancel(taskId: taskId)
            case .decide(let requestId, let approved, let category, let deviceId):
                decide(requestId: requestId, approved: approved, category: category, deviceId: deviceId)
            case .devicePaired(let device):
                pairing = nil
                note("Paired \u{201C}\(device.name)\u{201D}", device: device.id, warning: false)
            case .audit(let entry):
                record(entry)
            }
        }
    }

    private func closeConnection(_ id: UInt64) {
        open.remove(id)
        connected[id] = nil
        transport?.close(id)
    }

    private func startRemote(taskId: UInt64, deviceId: String, text: String, skill: SkillContext?) {
        let origin = TaskOrigin.remote(taskId: taskId, deviceId: deviceId, deviceName: deviceName(deviceId))
        if case .failure(let refusal) = center.start(text, skill: skill, origin: origin) {
            enqueue(.local(.sessionFinished(taskId: taskId, outcome: Self.failed(refusal.description))))
        }
    }

    /// Cancels only what `taskId` names (`nil`: the owner's own task or workflow).
    private func cancel(taskId: UInt64?) {
        if let workflow = center.workflow {
            if workflow.origin.remoteTaskId == taskId { runner.cancel() }
        } else if let session = center.session, session.origin.workflow == nil, session.origin.remoteTaskId == taskId {
            center.cancelSession()
        }
    }

    /// The broker prompt a newly announced `request` belongs to: the first confirmation
    /// prompt with that request shown after the last one bound. `nil` when it is not shown
    /// yet (the run announces the confirmation just before it asks the broker); it is bound
    /// when a decision arrives.
    private func bindPrompt(for request: ConfirmRequest) -> Int? {
        guard let prompt = desk.confirmations(after: boundPromptId).first(where: { $0.request == request }) else { return nil }
        boundPromptId = prompt.id
        return prompt.id
    }

    private func decide(requestId: UInt64, approved: Bool, category: RiskCategory, deviceId: String) {
        let who = deviceName(deviceId) ?? "a device"
        if var pending = hubConfirm, pending.id == requestId, pending.promptId == nil {
            pending.promptId = bindPrompt(for: pending.request)
            hubConfirm = pending
        }
        // Bound to one prompt: only that prompt, and only while it is the one on screen.
        guard let pending = hubConfirm, pending.id == requestId, let promptId = pending.promptId,
              let onScreen = desk.pendingConfirmation, onScreen.id == promptId,
              pending.request.category == category, onScreen.request == pending.request else {
            note("Ignored a stale decision from \(who): the question it answers is no longer the one on the phone",
                 device: deviceId, warning: true)
            return
        }
        if approved, (try? remotelyApprovable(category: category)) != true {
            note("Refused an approval from \(who): \(riskCategoryTitle(category)) can only be approved on the phone",
                 device: deviceId, warning: true)
            return
        }
        desk.resolveConfirmation(onScreen.id, approved: approved)
        note("\(approved ? "Approved" : "Declined") \(riskCategoryTitle(category).lowercased()) from \(who)", device: deviceId, warning: false)
    }

    // MARK: - What happens on the phone

    private func taskEvent(_ event: TaskCenter.Event) {
        guard hub != nil else { return }
        switch event {
        case .sessionStarted(let session):
            lastScreen = nil
            if session.origin.workflow == nil {
                enqueue(.local(.sessionStarted(taskId: session.origin.remoteTaskId)))
            }
        case .session(_, .update(let update)):
            switch update {
            case .effectCompleted(.observe, .observed(let observation)):
                lastScreen = observation.screen
            case .effectCompleted(.observe, .observedRunnerText(let snapshot)):
                lastScreen = try? parseRunnerTextToScreen(snapshot: snapshot)
            case .progress(let progress):
                // The typed screen: the core builds and redacts any text remotes see.
                enqueue(.local(.progress(progress: progress, screen: lastScreen)))
            case .confirmationPending(let request):
                enqueue(.confirmRequested(request))
            case .effectCompleted(.confirm, let decision):
                enqueue(.confirmResolved(approved: decision == .userDecision(approved: true)))
            default:
                break
            }
        case .session:
            break
        case .sessionFinished(let session, let outcome):
            enqueue(.confirmResolved(approved: false))
            if session.origin.workflow == nil {
                enqueue(.local(.sessionFinished(taskId: session.origin.remoteTaskId, outcome: outcome.outcome)))
            }
        case .workflowStarted(let slot):
            enqueue(.local(.sessionStarted(taskId: slot.origin.remoteTaskId)))
        case .workflowFinished(let slot, let outcome):
            enqueue(.local(.sessionFinished(taskId: slot.origin.remoteTaskId, outcome: Self.outcome(outcome))))
        }
    }

    /// A workflow's outcome in a session's terms (a stop by condition reads Cancelled).
    static func outcome(_ w: WorkflowOutcome) -> Outcome {
        let status: OutcomeStatus = switch w.status {
        case .succeeded: .success
        case .completedWithFailures, .failed: .failed
        case .stopped, .cancelled: .cancelled
        case .blocked: .blocked
        case .interrupted: .interrupted
        }
        return Outcome(status: status, summary: w.summary, steps: w.stepsRun, durationMs: 0)
    }

    static func failed(_ why: String) -> Outcome {
        Outcome(status: .failed, summary: why, steps: 0, durationMs: 0)
    }

    // MARK: - Internals

    /// Maps a hub error; a poisoned hub is rebuilt from the secret store.
    @discardableResult
    private func handleHubError(_ error: any Error) -> RemoteControlError {
        if CoreErrorMessage(error).isPoisoned {
            rebuild()
            return .core("Remote control restarted after an internal error.")
        }
        let failure = RemoteControlError(core: error)
        if case .randomSource = failure { lastError = failure.description }
        return failure
    }

    /// Replaces a poisoned hub with one loaded from the secret store. Connections the old
    /// hub knew are closed (the new one has never seen them).
    private func rebuild() {
        for id in open.sorted() { transport?.close(id) }
        open = []
        connected = [:]
        pairing = nil
        hubConfirm = nil
        hub = nil
        lastError = "Remote control restarted after an internal error; devices reconnect by themselves."
        note("Remote control restarted after an internal error", device: nil, warning: true)
        if loadHub() {
            lastCatalog = nil
            refreshCatalog()
        } else {
            stop()
        }
    }

    private func record(_ entry: AuditEntry) {
        if case .connected = entry.event, let conn = entry.connId, let device = entry.deviceId {
            connected[conn] = device
        }
        if case .closed = entry.event, let conn = entry.connId { connected[conn] = nil }
        let (text, warning) = RemoteAuditText.describe(entry.event)
        audit.append(RemoteAuditRecord(at: Date(timeIntervalSince1970: Double(entry.atMs) / 1_000),
                                       device: entry.deviceId.map { deviceName($0) ?? Self.shortId($0) },
                                       text: text, isWarning: warning))
    }

    private func note(_ text: String, device: String?, warning: Bool) {
        audit.append(RemoteAuditRecord(at: Date(timeIntervalSince1970: Double(clock()) / 1_000),
                                       device: device.map { deviceName($0) ?? Self.shortId($0) },
                                       text: text, isWarning: warning))
    }

    static func shortId(_ id: String) -> String { "device \(id.prefix(6))" }

    private func refreshDevices() {
        devices = (try? hub?.devices()) ?? []
    }

    private func changed() { onChange?() }
}
