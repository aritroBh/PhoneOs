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

/// A hub the Live view can stream through: `JarvisViewer.feedAndStream` takes the core's own
/// hub object (the timeline never crosses the FFI). The core's `JarvisRemoteHub` is one;
/// a test wrapper around it says which hub it wraps.
public protocol LiveViewStreamingHub {
    var liveViewHub: JarvisRemoteHub { get }
}

extension JarvisRemoteHub: LiveViewStreamingHub {
    public var liveViewHub: JarvisRemoteHub { self }
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
///   If the write fails, nothing after it goes out, every connection is closed, and the hub
///   is told (`storeWriteFailed`, whose closes it hands back are carried out so it forgets
///   the connections): the question pending then stays the phone's own, the next request
///   id is leased (and stored) again before any device sees it, and every later hub call
///   (the tick at the latest) asks for the write again until it succeeds. Meanwhile
///   `changesUnsaved` is set: remote control is paused, no device connects.
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
///   `remotelyApprovable(category:)` and a request that is not `phoneOnly` (thirteenth review).
/// - **Lifecycle.** Off by default; on starts the listener and a tick timer; off stops both
///   and drops the hub. A poisoned hub is rebuilt from the secret store.
/// - **Live view stream.** `feedLiveView(_:_:)` is the phone's one FFI call per driver
///   update while remote control is on (`JarvisViewer.feedAndStream`: feeds the Live view
///   and hands the hub its timeline); the frames it returns go out through the same ordered
///   path as every hub effect, ahead of hub calls not made yet. Every frame is sent with a
///   write completion that is reported back (`written`), so a device that stops reading
///   never makes the phone queue more than the hub's bound; `tick` sends screens held back
///   by the hub's rate limit. Only devices with "Watch the screen layout" (off by default)
///   get anything; turning it off ends their stream in the same call (audited).
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
    /// Devices paired with a code but not yet confirmed by the owner here: they hold no
    /// permission. Each shows its name and check string with Confirm and Remove; the owner
    /// confirms only when the device they paired shows the same string. One not confirmed
    /// within 10 minutes is removed by the hub.
    public private(set) var pendingDevices: [PendingDevice] = []
    /// Wrong codes tried against the open pairing offer (0 without one). Above zero the
    /// pairing sheet says that wrong codes are being tried from the network.
    public var pairingFailedAttempts: UInt32 { (try? hub?.pairingFailedAttempts()) ?? 0 }
    public private(set) var pairing: RemotePairingOffer?
    /// The last problem worth showing (refused entropy, a failed write, a rebuilt hub).
    public private(set) var lastError: String?
    /// A change could not be saved (the Keychain write failed) and no write has succeeded
    /// since: every connection was closed and remote control is paused (no device connects)
    /// until the hub's state is saved; the write is tried again every tick. The Remote
    /// control screen says so (`unsavedMessage`).
    public private(set) var changesUnsaved = false
    /// What the Remote control screen shows while `changesUnsaved`.
    public static let unsavedMessage = "Changes could not be saved; remote control is paused until they are."
    /// Connections that finished their handshake, by connection id → device id.
    public private(set) var connected: [UInt64: String] = [:]
    /// Devices that watch the Live view now (on at least one connection).
    public private(set) var watching: Set<String> = []
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
    /// Write completions from the transport (any thread), reported to the hub in order.
    private var writtenSink: AsyncStream<(UInt64, Int)>.Continuation?
    private var writtenConsumer: Task<Void, Never>?
    /// Completed bytes per connection not yet reported to the hub.
    private var unreported: [UInt64: Int] = [:]
    private var reportScheduled = false
    private var open: Set<UInt64> = []
    private var observer: UUID?

    private enum Input {
        case hub(@MainActor (any JarvisRemoteHubProtocol) throws -> [HubEffect])
        case confirmRequested(ConfirmRequest)
        case confirmResolved(approved: Bool)
        case local(LocalEvent)
    }

    private var inbox: [Input] = []
    /// Effects the hub already returned outside the inbox (a Live view feed, an owner
    /// action): carried out before any inbox entry, whose hub call comes later.
    private var ready: [[HubEffect]] = []
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
        let (written, sink) = AsyncStream.makeStream(of: (UInt64, Int).self)
        writtenSink = sink
        writtenConsumer = Task { @MainActor [weak self] in
            for await (id, count) in written {
                self?.reportWritten(id, count)
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
        // A change not saved yet gets one last try before the hub is dropped.
        if changesUnsaved {
            changesUnsaved = false
            if let json = try? hub.toJson(), (try? secrets.write(Self.secretKey, json)) != nil {
                if lastError == Self.unsavedMessage { lastError = nil }
            } else {
                lastError = "Changes could not be saved before remote control was turned off; the state saved last will load."
            }
        }
        consumer?.cancel()
        ticker?.cancel()
        consumer = nil
        ticker = nil
        writtenSink?.finish()
        writtenSink = nil
        writtenConsumer?.cancel()
        writtenConsumer = nil
        unreported = [:]
        reportScheduled = false
        transport?.stop()
        transport = nil
        open = []
        connected = [:]
        watching = []
        pairing = nil
        hubConfirm = nil
        inbox = []
        ready = []
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

    /// Removes a device at once (a paired one, or one waiting for confirmation): its key is
    /// forgotten, its connections close and the task it asked for stops (never one the
    /// owner started).
    public func revoke(deviceId: String) throws {
        let pending = pendingDevices.first { $0.device.id == deviceId }
        try ownerAction { try $0.revokeDevice(nowMs: self.clock(), deviceId: deviceId) }
        if let pending {
            note("Removed \u{201C}\(pending.device.name)\u{201D} before it was confirmed", device: nil, warning: false)
        }
    }

    /// The owner confirms on this phone a device waiting for confirmation, after checking
    /// that the device they paired shows the same check string. It then gets the new-device
    /// permissions (start and stop tasks, see that a task runs); everything else stays off.
    public func confirm(deviceId: String) throws {
        try ownerAction { try $0.confirmDevice(nowMs: self.clock(), deviceId: deviceId) }
    }

    /// The check string shown beside a device waiting for confirmation.
    public func pendingCheck(_ deviceId: String) -> String? {
        pendingDevices.first { $0.device.id == deviceId }?.check
    }

    public func isConnected(_ deviceId: String) -> Bool { connected.values.contains(deviceId) }

    /// Where a controller reaches this phone now (the Remote control screen's connection
    /// details): the listener's local addresses and port; empty while not listening.
    public var connectionDetails: [RemotePhoneAddress] {
        guard case .listening = status else { return [] }
        return transport?.localAddresses() ?? []
    }

    /// "Watch the screen layout" for a device: it may see the Live view (outlines of the
    /// screen, where Jarvis taps, step captions; never pictures of the screen, passwords
    /// or the lock screen). Off by default; stored with the device. Off ends its stream at
    /// once. Never changes what may be approved.
    public func setWatchScreen(deviceId: String, _ allowed: Bool) throws {
        try ownerAction { try $0.setWatchScreen(nowMs: self.clock(), deviceId: deviceId, allowed: allowed) }
    }

    /// Whether a device may watch the screen layout.
    public func watchScreen(_ deviceId: String) -> Bool { devices.first { $0.id == deviceId }?.watchScreen ?? false }

    /// Whether a device watches the Live view now.
    public func isWatching(_ deviceId: String) -> Bool { watching.contains(deviceId) }

    // MARK: - Live view stream

    /// The phone's Live view takes one driver update: ONE FFI call that feeds `viewer` and,
    /// while remote control is on, streams to the devices that watch
    /// (`JarvisViewer.feedAndStream`). Returns whether `viewer`'s timeline or approval
    /// picture changed (as `LiveViewModel.apply(_:)`). Off, or with a hub that cannot
    /// stream, it is a plain `apply`.
    @discardableResult
    public func feedLiveView(_ viewer: LiveViewModel, _ event: RunEvent) -> Bool {
        guard let streaming = (hub as? any LiveViewStreamingHub)?.liveViewHub else { return viewer.apply(event) }
        let streamed = viewer.apply(event, streamingTo: streaming, nowMs: clock())
        guard let fed = streamed.fed else { return streamed.changed }
        if let why = fed.hubError {
            handleHubError(JarvisError.Poisoned(message: why))
            changed()
        } else if !fed.effects.isEmpty {
            deliverComputed(fed.effects)
        }
        return streamed.changed
    }

    /// The transport wrote `count` bytes of a connection's frames: reported to the hub
    /// (coalesced per connection) through the inbox; frames waiting for room follow.
    private func reportWritten(_ id: UInt64, _ count: Int) {
        guard hub != nil, open.contains(id), count > 0 else { return }
        unreported[id, default: 0] += count
        guard !reportScheduled else { return }
        reportScheduled = true
        enqueue(.hub { [weak self] hub in
            guard let self else { return [] }
            self.reportScheduled = false
            let batch = self.unreported.sorted { $0.key < $1.key }
            self.unreported = [:]
            var effects: [HubEffect] = []
            for (id, count) in batch where self.open.contains(id) {
                effects += try hub.written(connId: id, bytes: UInt64(count), nowMs: self.clock())
            }
            return effects
        })
    }

    public func deviceName(_ id: String) -> String? {
        devices.first { $0.id == id }?.name ?? pendingDevices.first { $0.device.id == id }?.device.name
    }

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
        deliverComputed(effects)
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
        drain()
    }

    /// Effects a hub call outside the inbox returned. Its frames are sealed after those of
    /// every hub call made so far and before those of any inbox entry (whose call comes
    /// later), so they go out after the effects being carried out now and before the inbox.
    private func deliverComputed(_ effects: [HubEffect]) {
        ready.append(effects)
        drain()
    }

    private func drain() {
        guard !working else { return }
        working = true
        while !ready.isEmpty || !inbox.isEmpty {
            guard let hub else { inbox = []; ready = []; break }
            if !ready.isEmpty {
                carryOut(ready.removeFirst())
                continue
            }
            let next = inbox.removeFirst()
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
                    if changesUnsaved {
                        changesUnsaved = false
                        if lastError == Self.unsavedMessage { lastError = nil }
                    }
                } catch {
                    persistFailed = true
                    storeWriteFailed()
                }
            case .send(let id, let bytes):
                // The completion reports the write to the hub (Live view backpressure).
                let sink = writtenSink
                transport?.send(bytes, to: id) { count in sink?.yield((id, count)) }
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
                refreshDevices()
                let check = pendingCheck(device.id).map { " (it shows \($0))" } ?? ""
                note("Paired \u{201C}\(device.name)\u{201D}\(check): waiting for you to confirm it on this phone",
                     device: nil, warning: false)
            case .deviceConfirmed(let device):
                refreshDevices()
                note("Confirmed \u{201C}\(device.name)\u{201D} on this phone", device: device.id, warning: false)
            case .audit(let entry):
                record(entry)
            }
        }
    }

    /// A store write failed. The hub's leases may now be ahead of what is stored: it keeps
    /// the pending question on the phone and leases again before the next id goes out, so
    /// no request id from an unstored lease reaches a device (and none repeats after a
    /// restart); the phone still asks locally. It closes every connection it had (carried
    /// out here, so it forgets them: second break-it review), and asks for the write again
    /// at its next call (the tick, every second) until one succeeds. Until then remote
    /// control is paused (`changesUnsaved`): no device stays connected, and none gets in,
    /// since the write a new connection's call starts with comes before its hello. If the
    /// app is killed before any write succeeds, the state stored last loads.
    private func storeWriteFailed() {
        changesUnsaved = true
        lastError = Self.unsavedMessage
        let effects = (try? hub?.storeWriteFailed()) ?? []
        for effect in effects {
            switch effect {
            case .close(let id, _): closeConnection(id)
            case .audit(let entry): record(entry)
            default: break
            }
        }
        // What is left open the hub has already forgotten (it closed it in this same call
        // list, a `close` not carried out yet): closed here. No other hub call here: its
        // effects (the write the hub asks for again first) would be dropped.
        for id in open.sorted() { closeConnection(id) }
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
        // Thirteenth review: a phone-only request (a presumed submit, a link the owner did
        // not give) is never approved remotely, whatever its category.
        if approved, pending.request.phoneOnly || (try? remotelyApprovable(category: category)) != true {
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
            // Sixteenth review (F9): no lock-screen content ever goes to a remote device. A
            // screen observed while the phone was locked is never kept (the last one is
            // dropped too), and none is sent while the core says the latest observation
            // was locked or it waits for an unlock (the hub drops it again on the same
            // flags).
            case .effectCompleted(.observe, .observed(let observation)):
                lastScreen = observation.deviceLocked ? nil : observation.screen
            case .effectCompleted(.observe, .observedRunnerText(let snapshot)):
                lastScreen = snapshot.deviceLocked ? nil : (try? parseRunnerTextToScreen(snapshot: snapshot))
            case .effectCompleted(_, .deviceLocked):
                lastScreen = nil
            case .progress(let progress):
                // The typed screen: the core builds and redacts any text remotes see.
                // Seventeenth review: nor Jarvis's own app while the run does not drive it
                // (the core shows the model a stub of it; a remote sees no more).
                let withheld = progress.screenLocked || progress.waitingForUnlock || progress.screenHidden
                if withheld { lastScreen = nil }
                enqueue(.local(.progress(progress: progress, screen: withheld ? nil : lastScreen)))
            case .confirmationPending(let request):
                enqueue(.confirmRequested(request))
            case .effectCompleted(.confirm, let decision):
                let approved = if case .userDecision(approved: true, _) = decision { true } else { false }
                enqueue(.confirmResolved(approved: approved))
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
        watching = []
        unreported = [:]
        reportScheduled = false
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
        pendingDevices = (try? hub?.pendingDevices()) ?? []
        var now: Set<String> = []
        if let hub {
            for (conn, device) in connected where (try? hub.isWatching(connId: conn)) == true { now.insert(device) }
        }
        watching = now
    }

    private func changed() { onChange?() }
}
