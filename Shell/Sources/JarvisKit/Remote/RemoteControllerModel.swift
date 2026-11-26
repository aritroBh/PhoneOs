import Foundation
import JarvisCore

/// What can go wrong on the controller, in the owner's words.
public enum RemoteControllerError: Error, Sendable, Equatable, CustomStringConvertible {
    /// Not connected to the phone now.
    case notConnected
    /// The phone has not allowed this device to do that (nothing was sent).
    case notAllowed(String)
    /// The code typed is not a pairing code (nothing was sent).
    case invalidCode(String)
    case invalidAddress(String)
    /// Already paired (forget the phone first) or a pairing is under way.
    case busy(String)
    /// The pairing could not be kept (the device's key store refused it).
    case store(String)
    case randomSource(String)
    case core(String)

    public var description: String {
        switch self {
        case .notConnected: "Not connected to the phone."
        case .notAllowed(let why): why
        case .invalidCode(let why): "That is not a pairing code (\(why)). It is 32 letters and digits, shown on the phone."
        case .invalidAddress(let why): why
        case .busy(let why): why
        case .store(let why): "The pairing could not be saved on this device (\(why))."
        case .randomSource(let why): "This device's random source failed (\(why)). Nothing was sent."
        case .core(let why): why
        }
    }
}

/// The controller's side of remote control: this device pairs with another phone that runs
/// Jarvis, starts and stops tasks there, follows their status and, where the phone allows
/// it, answers its questions and watches its Live view. Everything shown comes from what the
/// phone sent; the phone is the authority for every permission and refuses whatever it did
/// not allow. This model offers only what the phone said this device may do, and survives a
/// refusal (it is shown, never fatal).
///
/// - **States** (`State.phase`): not paired → pairing (the code shown on the phone; the
///   protocol has no short authentication string to compare: the 160-bit code itself is
///   the secret, single use, 5 minutes) → connecting → connected → disconnected (with a
///   reason; reconnects by itself with a bounded backoff, `ControllerReconnectPolicy`).
///   While connected the task is unknown (the phone shares nothing about it), idle,
///   running (possibly waiting for an answer or an unlock), or ended with its outcome.
/// - **Pairing is persisted** in the `SecretStore` (the app: the Keychain) under
///   `secretKey`: the core's credentials (the device key), the phone's name and address.
///   Never printed (`description` is redacted), never anywhere else.
/// - **Live view**: `live` follows the phone's stream ("Watch the screen layout", off by
///   default on the phone) into its own `LiveViewModel`; the protocol does not tell a device
///   whether it may watch, so the owner asks (`startWatching`) and the phone answers with a
///   snapshot or a refusal. After a reconnect a stream that was on is asked for again, and
///   the phone answers with a snapshot (resync).
@MainActor
public final class RemoteControllerModel {
    public static let secretKey = "remote-controller"
    /// Status lines kept.
    public static let maxEntries = 100
    /// Messages kept in `received` (tests read them).
    static let maxReceived = 2_000

    public enum Phase: Sendable, Equatable {
        case notPaired
        case pairing
        /// Opening a connection with the stored pairing; `attempt` 0 is the first.
        case connecting(attempt: Int)
        case connected
        case disconnected(Disconnect)
    }

    public struct Disconnect: Sendable, Equatable {
        public var reason: DisconnectReason
        /// The controller tries again by itself (after the policy's delay).
        public var retrying: Bool
        /// Attempts made since the connection was last up.
        public var attempts: Int
    }

    public enum DisconnectReason: Sendable, Equatable {
        /// The connection went (the phone closed it, Jarvis was closed there, the network).
        case connectionLost
        /// Nothing answered at the address.
        case unreachable(String)
        /// The phone closed the connection before the handshake finished, more than once:
        /// it no longer accepts this device (it was removed there, or it is paused after
        /// failed attempts).
        case refused
        /// The retries were used up; the owner can try again.
        case gaveUp
        /// The owner disconnected.
        case byOwner

        public var text: String {
            switch self {
            case .connectionLost: "The connection to the phone was lost."
            case .unreachable(let why): "The phone could not be reached: \(why)"
            case .refused: "The phone no longer accepts this device. Its owner may have removed it; pair again."
            case .gaveUp: "Could not reconnect. Make sure Jarvis is open on the phone, on the same Wi-Fi, with Remote control on."
            case .byOwner: "Disconnected."
            }
        }
    }

    public enum Activity: Sendable, Equatable {
        /// The phone shares nothing about its tasks with this device (or has not said yet).
        case unknown
        case idle
        case running
        case ended(Ended)
    }

    public struct Ended: Sendable, Equatable {
        public var status: OutcomeStatus
        /// The phone's summary: its own words with "See what Jarvis sees", a generic line
        /// otherwise.
        public var summary: String
    }

    /// The phone's pending question as it was sent to this device.
    public struct Question: Sendable, Equatable, Identifiable {
        public var requestId: UInt64
        public var category: RiskCategory
        /// Redacted; generic unless this device may see screen text or approve it.
        public var summary: String
        /// The phone said an answer from this device would be honoured. False for a
        /// question only the phone can answer (payment, purchase, credentials, account
        /// change, delete, unreadable controls, any phone-only question) or when this device
        /// may not approve: then it is answered on the phone.
        public var canApprove: Bool
        public var id: UInt64 { requestId }
    }

    /// The last progress the phone sent (detail only with "See what Jarvis sees").
    public struct Progress: Sendable, Equatable {
        public var step: UInt32
        public var phase: JarvisCore.Phase
        public var app: String?
        public var lastAction: String?
        public var screenText: String?
        public var waitingForUnlock: Bool
        /// The phone sent only that a task runs or waits (no step, app or action).
        public var coarse: Bool { step == 0 && app == nil && lastAction == nil && screenText == nil }
    }

    /// One line of the status timeline.
    public struct Entry: Sendable, Equatable, Identifiable {
        public enum Kind: Sendable, Equatable { case connection, task, progress, question, outcome, refusal }
        public var id: Int
        public var kind: Kind
        public var text: String
    }

    /// Everything the controller screen shows, as one value.
    public struct State: Sendable, Equatable {
        public var phase: Phase = .notPaired
        /// The phone's name as it sent it (absent while it shares nothing with this device).
        public var phoneName: String?
        public var address: RemotePhoneAddress?
        /// What the phone says this device may do (`nil` until it said).
        public var permissions: RemotePermissions?
        public var activity: Activity = .unknown
        public var progress: Progress?
        public var question: Question?
        public var entries: [Entry] = []
        /// A problem on this side (a code that is not one, an unreachable address).
        public var lastError: String?
        /// The phone's last refusal, in words.
        public var lastRefusal: String?
        public var live: RemoteLiveView.State = .idle
        /// The owner wants the Live view (asked again after a reconnect).
        public var watchWanted = false

        /// Public so the app can hold an empty state before the model exists (Mac run 160:
        /// the synthesized initializer is internal).
        public init() {}

        public var isPaired: Bool {
            switch phase {
            case .notPaired, .pairing: false
            default: true
            }
        }

        public var isConnected: Bool { phase == .connected }

        public var isBusy: Bool {
            if case .running = activity { return true }
            return false
        }

        /// The phone allows this device to start tasks, and none runs.
        public var canStartTask: Bool { isConnected && permissions?.startTasks == true && !isBusy }
        /// Stopping is allowed for every paired device; offered while a task runs.
        public var canStop: Bool { isConnected && isBusy }
        /// The pending question can be answered here (the phone said so).
        public var canAnswer: Bool { isConnected && question?.canApprove == true }
        public var canWatch: Bool {
            guard isConnected else { return false }
            switch live {
            case .watching, .starting: return false
            case .incompatible: return false
            default: return true
            }
        }

        /// What this phone does not share with this device, in words (`nil`: everything
        /// this screen shows).
        public var sharingNote: String? {
            guard let p = permissions else { return nil }
            if p == RemotePermissions(startTasks: false, seeProgress: false, seeScreenText: false, approve: false) {
                return "This phone shares nothing with this device. Its owner can allow more in Remote control on the phone."
            }
            if !p.seeProgress {
                return "This phone does not share its tasks with this device: only those you start here, and how they ended."
            }
            if !p.seeScreenText {
                return "This phone shares only that a task is running, when it waits, and how it ended."
            }
            return nil
        }

        /// Words for the connection and the task (the status line).
        public var statusTitle: String {
            switch phase {
            case .notPaired: return "Not paired"
            case .pairing: return "Pairing\u{2026}"
            case .connecting(let attempt): return attempt == 0 ? "Connecting\u{2026}" : "Reconnecting\u{2026}"
            case .disconnected(let d): return d.retrying ? "Reconnecting\u{2026}" : "Disconnected"
            case .connected: break
            }
            if question != nil { return "Waiting for an answer" }
            switch activity {
            case .unknown: return "Connected"
            case .idle: return "Idle"
            case .running: return progress?.waitingForUnlock == true ? "Waiting for the phone to be unlocked" : "Task running"
            case .ended(let e): return "Ended \u{00B7} \(Self.title(e.status))"
            }
        }

        public static func title(_ status: OutcomeStatus) -> String {
            switch status {
            case .success: "Done"
            case .failed: "Failed"
            case .cancelled: "Stopped"
            case .blocked: "Blocked"
            case .interrupted: "Phone stayed locked"
            }
        }
    }

    public private(set) var state = State()
    /// The Live view of the phone (its own viewer; wireframes only).
    public private(set) var live = RemoteLiveView()
    /// Called after every change.
    public var onChange: (@MainActor () -> Void)?

    /// Every message the phone sent, in order (bounded): what the tests check reached this
    /// device.
    private(set) var received: [ServerMessage] = []

    private let secrets: any SecretStore
    private let random: any RandomSource
    private let dialer: any RemoteDialer
    private let deviceName: String
    private let policy: ControllerReconnectPolicy
    private let keepAlive: Duration
    private let handshakeTimeout: Duration
    private let sleep: @Sendable (Duration) async throws -> Void

    private enum Intent { case pair, connect }

    /// One connection attempt and its connection.
    private struct Link {
        let generation: Int
        let intent: Intent
        let client: JarvisRemoteClient
        var connection: (any RemoteConnection)?
        var authenticated = false
        var subscribed = false
    }

    private var link: Link?
    private var generation = 0
    private var record: PairedPhone?
    private var attempts = 0
    private var refusals = 0
    private var retry: Task<Void, Never>?
    private var pinger: Task<Void, Never>?
    private var waiters: [CheckedContinuation<Void, Never>] = []
    private var nextEntry = 0

    public init(
        secrets: any SecretStore,
        random: any RandomSource,
        dialer: any RemoteDialer,
        deviceName: String,
        policy: ControllerReconnectPolicy = ControllerReconnectPolicy(),
        keepAlive: Duration = .seconds(30),
        handshakeTimeout: Duration = .seconds(15),
        sleep: (@Sendable (Duration) async throws -> Void)? = nil
    ) {
        self.secrets = secrets
        self.random = random
        self.dialer = dialer
        self.deviceName = deviceName
        self.policy = policy
        self.keepAlive = keepAlive
        self.handshakeTimeout = handshakeTimeout
        self.sleep = sleep ?? { try await Task.sleep(for: $0) }
    }

    // MARK: - Pairing

    /// Reads the stored pairing (once) and connects with it if not connected. The screen
    /// calls it when it appears. A stored pairing that cannot be read is reported and kept.
    public func activate() {
        if record == nil {
            do {
                if let json = try secrets.read(Self.secretKey) {
                    record = try PairedPhone.decode(json)
                    state.phoneName = record?.phoneName
                    state.address = record?.address
                }
            } catch {
                state.lastError = "The saved pairing could not be read (\(error))."
                changed()
                return
            }
        }
        guard record != nil else {
            if state.phase != .pairing { state.phase = .notPaired }
            changed()
            return
        }
        // A stored pairing connects by itself once; after a refusal, the retries running
        // out or the owner's disconnect, only Reconnect tries again.
        if state.phase == .notPaired { connectNow() }
    }

    /// Pairs with the phone at `address` using the code shown on its screen (typed or
    /// scanned). Returns once paired and connected, or once it failed (`state.lastError`).
    /// A wrong code is refused by the phone the same way as any other failure (the protocol
    /// says nothing more); after 5 wrong codes the phone pauses pairing for a while. Nothing
    /// is retried by itself.
    public func pair(code: Redacted<String>, address: RemotePhoneAddress) async throws {
        guard record == nil else { throw RemoteControllerError.busy("This device is already paired with a phone. Forget it first.") }
        guard state.phase == .notPaired else { throw RemoteControllerError.busy("A pairing is under way.") }
        let entropy = try entropyBytes()
        let client: JarvisRemoteClient
        do {
            client = try JarvisRemoteClient.pair(code: code.reveal(), deviceName: deviceName, entropy: entropy)
        } catch let error as JarvisError {
            if case .RemoteInvalidCode(let message) = error { throw fail(.invalidCode(message)) }
            if case .RemoteInvalidName = error { throw fail(.core("This device's name cannot be used for pairing.")) }
            throw fail(.core(CoreErrorMessage(error).message))
        }
        state.lastError = nil
        state.lastRefusal = nil
        state.address = address
        state.phase = .pairing
        changed()
        await open(client, intent: .pair, address: address)
        if state.phase == .notPaired, let why = state.lastError { throw RemoteControllerError.core(why) }
    }

    /// Opens a connection again now (the owner tapped Reconnect), with fresh attempts.
    public func reconnect() {
        guard record != nil else { return }
        attempts = 0
        refusals = 0
        connectNow()
    }

    /// Closes the connection; stays paired.
    public func disconnect() {
        retry?.cancel()
        retry = nil
        drop()
        if record != nil {
            state.phase = .disconnected(Disconnect(reason: .byOwner, retrying: false, attempts: 0))
            note(.connection, "Disconnected")
        }
        changed()
    }

    /// Forgets the phone: closes the connection and deletes the stored pairing. The phone
    /// still lists this device until its owner removes it there.
    public func forget() {
        retry?.cancel()
        retry = nil
        drop()
        try? secrets.delete(Self.secretKey)
        record = nil
        live = RemoteLiveView()
        state = State()
        changed()
    }

    // MARK: - Commands (only what the phone allowed; the phone checks again)

    /// Starts a task on the phone. Needs the phone's "Start and stop tasks" and no task
    /// running there.
    public func startTask(_ text: String) throws {
        let task = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard state.isConnected else { throw RemoteControllerError.notConnected }
        guard state.permissions?.startTasks == true else {
            throw RemoteControllerError.notAllowed("The phone does not allow this device to start tasks.")
        }
        guard !state.isBusy else { throw RemoteControllerError.notAllowed("A task is already running on the phone.") }
        guard !task.isEmpty else { throw RemoteControllerError.notAllowed("Say what to do.") }
        try send(.startTask(text: task))
        state.lastRefusal = nil
        note(.task, "Asked the phone to start a task")
        changed()
    }

    /// Stops the task running on the phone (every paired device may).
    public func stop() throws {
        guard state.isConnected else { throw RemoteControllerError.notConnected }
        guard state.isBusy else { throw RemoteControllerError.notAllowed("No task is running.") }
        try send(.cancel)
        note(.task, "Asked the phone to stop the task")
        changed()
    }

    /// Answers the pending question, only where the phone said this device may (never a
    /// payment, purchase, credentials, account change, delete, unreadable control or any
    /// phone-only question: those are answered on the phone).
    public func answer(approved: Bool) throws {
        guard state.isConnected else { throw RemoteControllerError.notConnected }
        guard let question = state.question else { throw RemoteControllerError.notAllowed("Nothing is being asked.") }
        guard question.canApprove else { throw RemoteControllerError.notAllowed("Answer this on the phone.") }
        try send(.decision(requestId: question.requestId, approved: approved))
        note(.question, approved ? "Approved from this device" : "Declined from this device")
        changed()
    }

    /// Asks the phone for its Live view (wireframes, cursor, captions; never pixels). The
    /// phone answers with a snapshot, or refuses when "Watch the screen layout" is off for
    /// this device there.
    public func startWatching() throws {
        guard state.isConnected, let client = link?.client, let connection = link?.connection else {
            throw RemoteControllerError.notConnected
        }
        let bytes: Data
        do {
            bytes = try live.start(client)
        } catch {
            throw RemoteControllerError.core(CoreErrorMessage(error).message)
        }
        connection.write(bytes)
        state.watchWanted = true
        state.live = live.state
        changed()
    }

    /// Stops the Live view (the last picture stays).
    public func stopWatching() throws {
        state.watchWanted = false
        guard let client = link?.client, let connection = link?.connection, state.isConnected else {
            state.live = .idle
            changed()
            return
        }
        if let bytes = try? live.stop(client) { connection.write(bytes) }
        state.live = live.state
        changed()
    }

    /// Test hook: sends `message` without the model's own checks (the phone must refuse
    /// what it did not allow; the model must show the refusal and carry on).
    func sendUnchecked(_ message: ClientMessage) throws {
        guard state.isConnected else { throw RemoteControllerError.notConnected }
        try send(message)
    }

    /// Test hook: the connection drops as the network would drop it (reported and
    /// reconnected like any loss).
    func dropConnectionForTesting() {
        link?.connection?.close()
    }

    // MARK: - Connections

    private func connectNow() {
        guard let record else { return }
        retry?.cancel()
        retry = nil
        drop()
        let entropy: Data
        let client: JarvisRemoteClient
        do {
            entropy = try entropyBytes()
            client = try JarvisRemoteClient.connect(credentialsJson: record.credentials, entropy: entropy)
        } catch {
            state.lastError = "The saved pairing cannot be used (\(CoreErrorMessage(error).message)). Forget the phone and pair again."
            state.phase = .disconnected(Disconnect(reason: .refused, retrying: false, attempts: attempts))
            changed()
            return
        }
        state.phase = .connecting(attempt: attempts)
        changed()
        let address = record.address
        Task { @MainActor [weak self] in await self?.open(client, intent: .connect, address: address) }
    }

    /// Dials, then follows the connection until it ends. Returns when the attempt resolved
    /// (connected, or failed).
    private func open(_ client: JarvisRemoteClient, intent: Intent, address: RemotePhoneAddress) async {
        generation += 1
        let generation = self.generation
        link = Link(generation: generation, intent: intent, client: client)
        let connection: any RemoteConnection
        do {
            connection = try await dialer.dial(address)
        } catch {
            guard self.generation == generation else { return }
            link = nil
            let why = (error as? RemoteDialError)?.description ?? String(describing: error)
            ended(generation: generation, intent: intent, authenticated: false, unreachable: why)
            return
        }
        guard self.generation == generation, link != nil else {
            connection.close()
            return
        }
        link?.connection = connection
        let timeout = handshakeTimeout
        let sleep = self.sleep
        Task { @MainActor [weak self] in
            try? await sleep(timeout)
            guard let self, self.generation == generation, self.link?.authenticated == false else { return }
            connection.close()
        }
        Task { @MainActor [weak self] in
            for await bytes in connection.incoming {
                guard let self, self.generation == generation else { return }
                self.receive(bytes)
                guard self.generation == generation, self.link != nil else { return }
            }
            guard let self, self.generation == generation else { return }
            let authenticated = self.link?.authenticated ?? false
            self.link = nil
            self.stopPinging()
            self.ended(generation: generation, intent: intent, authenticated: authenticated, unreachable: nil)
        }
        if intent == .pair {
            await withCheckedContinuation { waiters.append($0) }
        }
    }

    /// The attempt or the connection of `generation` ended.
    private func ended(generation: Int, intent: Intent, authenticated: Bool, unreachable: String?) {
        if state.live == .watching || state.live == .starting {
            // The stream went with the connection; asked again after a reconnect.
            state.live = .idle
        }
        if intent == .pair && record == nil {
            state.phase = .notPaired
            state.address = nil
            state.lastError = unreachable.map { "The phone could not be reached: \($0)" }
                ?? "The phone did not accept the code. Check it on the phone and try again; after 5 wrong codes the phone pauses pairing for a while."
            note(.connection, "Pairing failed")
            resume()
            changed()
            return
        }
        if authenticated {
            attempts = 0
            refusals = 0
            note(.connection, "The connection to the phone was lost")
        } else if unreachable == nil {
            refusals += 1
        }
        if refusals >= policy.maxRefusals {
            state.phase = .disconnected(Disconnect(reason: .refused, retrying: false, attempts: attempts))
            note(.connection, "The phone no longer accepts this device")
            resume()
            changed()
            return
        }
        let reason: DisconnectReason = unreachable.map { .unreachable($0) } ?? .connectionLost
        attempts += 1
        guard let delay = policy.delayMs(forAttempt: attempts) else {
            state.phase = .disconnected(Disconnect(reason: .gaveUp, retrying: false, attempts: attempts - 1))
            note(.connection, "Stopped reconnecting")
            resume()
            changed()
            return
        }
        state.phase = .disconnected(Disconnect(reason: reason, retrying: true, attempts: attempts))
        resume()
        changed()
        let sleep = self.sleep
        retry = Task { @MainActor [weak self] in
            do { try await sleep(.milliseconds(Int(clamping: delay))) } catch { return }
            guard let self, !Task.isCancelled, case .disconnected(let d) = self.state.phase, d.retrying else { return }
            self.connectNow()
        }
    }

    /// Closes the current connection without reporting it (a new attempt replaces it).
    private func drop() {
        generation += 1
        stopPinging()
        link?.connection?.close()
        link = nil
        if state.live == .watching || state.live == .starting { state.live = .idle }
        resume()
    }

    private func resume() {
        let pending = waiters
        waiters = []
        pending.forEach { $0.resume() }
    }

    private func startPinging() {
        stopPinging()
        let interval = keepAlive
        let sleep = self.sleep
        let generation = self.generation
        pinger = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                do { try await sleep(interval) } catch { return }
                guard let self, self.generation == generation, self.state.isConnected else { return }
                try? self.send(.ping)
            }
        }
    }

    private func stopPinging() {
        pinger?.cancel()
        pinger = nil
    }

    // MARK: - Bytes in

    private func receive(_ bytes: Data) {
        guard let current = link, let connection = current.connection else { return }
        let received: ClientReceived
        do {
            received = try current.client.receiveAll(bytes: bytes)
        } catch {
            // Final for this connection (a bad frame, a refused handshake).
            connection.close()
            return
        }
        for case .send(let out) in received.events { connection.write(out) }
        if !received.viewer.isEmpty {
            live.receive(received.viewer, client: current.client).forEach(connection.write)
            state.live = live.state
            switch live.state {
            case .denied, .ended, .incompatible: state.watchWanted = false
            default: break
            }
        }
        for event in received.events {
            switch event {
            case .send:
                break
            case .paired(let deviceId, let json):
                // Kept before anything else happens on this connection.
                let paired = PairedPhone(deviceId: deviceId, credentials: json, phoneName: nil,
                                         address: state.address ?? .inProcess)
                do {
                    try secrets.write(Self.secretKey, paired.encoded())
                    record = paired
                } catch {
                    state.lastError = RemoteControllerError.store(String(describing: error)).description
                    connection.close()
                    changed()
                    return
                }
            case .connected:
                guard link?.generation == current.generation else { return }
                link?.authenticated = true
                attempts = 0
                refusals = 0
                state.phase = .connected
                state.lastError = nil
                note(.connection, "Connected")
                // A pairing client says hello by itself (it confirms the pairing).
                if current.intent == .connect { try? send(.hello) }
                startPinging()
                resume()
            case .message(let message):
                handle(message)
            case .undecodable:
                note(.refusal, "The phone sent something this version does not understand")
            }
        }
        changed()
    }

    private func handle(_ message: ServerMessage) {
        received.append(message)
        if received.count > Self.maxReceived { received.removeFirst(received.count - Self.maxReceived) }
        switch message {
        case .welcome(_, let permissions, let phoneName, let busy):
            apply(permissions)
            if let phoneName, phoneName != state.phoneName {
                state.phoneName = phoneName
                if var r = record {
                    r.phoneName = phoneName
                    if (try? secrets.write(Self.secretKey, r.encoded())) != nil { record = r }
                }
            }
            if let busy {
                if busy, !state.isBusy { state.activity = .running }
                if !busy, state.isBusy {
                    state.activity = .idle
                    state.question = nil
                }
                if !busy, state.activity == .unknown { state.activity = .idle }
            }
            if state.watchWanted, state.live != .watching, state.live != .starting, let client = link?.client,
               let connection = link?.connection, let bytes = try? live.start(client) {
                // Asked again after a reconnect: the phone answers with a snapshot.
                connection.write(bytes)
                state.live = live.state
            }
        case .taskStarted:
            state.activity = .running
            state.progress = nil
            state.question = nil
            note(.task, "Task started")
        case .progress(let step, let phase, let app, let lastAction, let screenText, let waiting):
            let progress = Progress(step: step, phase: phase, app: app, lastAction: lastAction, screenText: screenText,
                                    waitingForUnlock: waiting)
            if !state.isBusy { state.activity = .running }
            let line = Self.line(progress)
            if line != Self.line(state.progress) { note(.progress, line) }
            state.progress = progress
        case .confirmPending(let requestId, let category, let summary, let canApprove):
            state.question = Question(requestId: requestId, category: category, summary: summary, canApprove: canApprove)
            note(.question, canApprove ? "Asks: \(summary)" : "Asks on the phone: \(summary)")
        case .confirmResolved(let requestId):
            if state.question?.requestId == requestId {
                state.question = nil
                note(.question, "Answered")
            }
        case .finished(let status, let summary):
            state.activity = .ended(Ended(status: status, summary: summary))
            state.question = nil
            state.progress = nil
            note(.outcome, "\(State.title(status)): \(summary)")
        case .skills:
            break
        case .permissionsChanged(let permissions):
            apply(permissions)
            note(.connection, "The phone changed what this device may do")
        case .denied(let reason):
            let text = Self.refusal(reason)
            state.lastRefusal = text
            note(.refusal, text)
        case .error(let code):
            let text = Self.refusal(code)
            state.lastRefusal = text
            note(.refusal, text)
        case .pong:
            break
        }
    }

    /// Takes the phone's word for what this device may do and subscribes only when it may
    /// see progress.
    private func apply(_ permissions: RemotePermissions) {
        state.permissions = permissions
        guard link != nil else { return }
        if permissions.seeProgress {
            if link?.subscribed == false {
                link?.subscribed = true
                try? send(.subscribe)
            }
        } else {
            // The phone stopped sending progress: what was shown may be stale.
            link?.subscribed = false
            state.progress = nil
            if state.question?.canApprove != true { state.question = nil }
        }
    }

    // MARK: - Internals

    private func send(_ message: ClientMessage) throws {
        guard let client = link?.client, let connection = link?.connection else { throw RemoteControllerError.notConnected }
        do {
            connection.write(try client.send(message: message))
        } catch {
            throw RemoteControllerError.core(CoreErrorMessage(error).message)
        }
    }

    private func entropyBytes() throws -> Data {
        do {
            return try random.bytes(count: Int(remoteMinEntropyLen()))
        } catch {
            throw fail(.randomSource(String(describing: error)))
        }
    }

    private func fail(_ error: RemoteControllerError) -> RemoteControllerError {
        state.lastError = error.description
        changed()
        return error
    }

    private func note(_ kind: Entry.Kind, _ text: String) {
        nextEntry += 1
        state.entries.append(Entry(id: nextEntry, kind: kind, text: text))
        if state.entries.count > Self.maxEntries { state.entries.removeFirst(state.entries.count - Self.maxEntries) }
    }

    private func changed() { onChange?() }

    static func line(_ p: Progress?) -> String {
        guard let p else { return "" }
        if p.waitingForUnlock { return "Waiting for the phone to be unlocked" }
        if p.coarse { return "Working" }
        var parts = ["Step \(p.step)", phaseTitle(p.phase)]
        if let app = p.app { parts.append(app) }
        if let action = p.lastAction { parts.append("last: \(action)") }
        return parts.joined(separator: " \u{00B7} ")
    }

    public static func phaseTitle(_ phase: JarvisCore.Phase) -> String {
        switch phase {
        case .idle: "Idle"
        case .observing: "Looking"
        case .thinking: "Thinking"
        case .acting: "Acting"
        case .verifying: "Checking"
        case .callingTool: "Using an app action"
        case .waitingForUser: "Waiting for an answer"
        case .finished: "Finished"
        case .waitingForUnlock: "Waiting for an unlock"
        }
    }

    static func refusal(_ reason: DenyReason) -> String {
        switch reason {
        case .notPermitted: "The phone refused: this device is not allowed to do that."
        case .busy: "The phone refused: a task is already running there."
        case .notRunning: "The phone refused: no task is running."
        case .staleRequest: "The phone refused: that question is no longer open."
        case .notRemotelyApprovable: "The phone refused: that can only be approved on the phone."
        case .unknownSkill: "The phone refused: it does not offer that skill."
        }
    }

    static func refusal(_ code: RemoteErrorCode) -> String {
        switch code {
        case .malformed: "The phone could not read a request."
        case .unknownMessage: "The phone does not know that request (it may run an older Jarvis)."
        case .rateLimited: "Too many requests at once; the phone dropped one."
        case .invalidRequest: "The phone refused a request that was empty or too long."
        case .tooLarge: "The phone's answer was too large."
        }
    }
}

/// The stored pairing. SECRET: `credentials` holds the device key; printing shows nothing.
struct PairedPhone: Codable, Sendable, Equatable, CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
    static let version = 1
    var version = PairedPhone.version
    var deviceId: String
    var credentials: String
    var phoneName: String?
    var address: RemotePhoneAddress

    init(deviceId: String, credentials: String, phoneName: String?, address: RemotePhoneAddress) {
        self.deviceId = deviceId
        self.credentials = credentials
        self.phoneName = phoneName
        self.address = address
    }

    var description: String { "PairedPhone(<redacted>)" }
    var debugDescription: String { description }
    var customMirror: Mirror { Mirror(self, children: [], displayStyle: .struct) }

    struct Unsupported: Error, CustomStringConvertible {
        var version: Int
        var description: String { "unsupported version \(version)" }
    }

    func encoded() throws -> String {
        String(decoding: try JSONEncoder().encode(self), as: UTF8.self)
    }

    static func decode(_ json: String) throws -> PairedPhone {
        let value = try JSONDecoder().decode(PairedPhone.self, from: Data(json.utf8))
        guard value.version == version else { throw Unsupported(version: value.version) }
        return value
    }
}
