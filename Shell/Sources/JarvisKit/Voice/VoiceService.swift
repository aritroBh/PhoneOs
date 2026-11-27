import Foundation
import JarvisCore

/// Voice on the phone: the core's `JarvisVoice` plus the IO it needs (`VoicePorts`).
///
/// - **The core decides, this carries out.** Every port event, owner action and task event
///   becomes a `VoiceEvent` in one FIFO inbox; each is stepped through the core with the
///   clock's `now`, and its effects are carried out in order (awaiting each port call)
///   before the next event is stepped. Nothing here reads a transcript.
/// - **One entry for tasks.** `StartTask` goes through `startTask` (the app passes the same
///   entry the Ask screen uses; the default is `TaskCenter.start`) with the origin the core
///   chose: `.voice` without a touch, `.owner` for a transcript confirmed by touch. `StopTask`
///   goes through `stopTask` (the app's Stop path; the default is
///   `TaskCenter.cancelSession`).
/// - **No spoken approval.** Nothing here answers a question: approvals stay with the
///   `HumanPromptBroker` and the owner's touch. While a question is pending, a transcript can
///   only stop the task (the core's decision) or is ignored.
/// - **Stale results are dropped.** Each recognition and each arming of the wake word has an
///   id; events of an earlier one are ignored.
/// - **Remote control.** Nothing voice-related goes to a paired device: a voice-started task
///   reaches remote control like any task started on the phone (through `TaskCenter`), and
///   this service talks to no hub.
@MainActor
public final class VoiceService {
    /// The transcript row on the phone.
    public struct Transcript: Sendable, Equatable {
        public var text: String
        /// It is the request, not a partial.
        public var isFinal: Bool
        /// It waits for the owner's touch on Start.
        public var needsConfirm: Bool
        /// The last partial taken at the endpoint timeout.
        public var endpointed: Bool

        public init(text: String, isFinal: Bool, needsConfirm: Bool, endpointed: Bool) {
            self.text = text
            self.isFinal = isFinal
            self.needsConfirm = needsConfirm
            self.endpointed = endpointed
        }
    }

    /// Starts a task (`text`, `origin`, and the id of voice's request, which the session must
    /// carry: `TaskCenter.start(_:skill:origin:voiceRequest:)`); returns the refusal when it
    /// did not start. The id stands for the core's request id (passed through as a token
    /// that is never reused, even after `stop()` replaces the core).
    public typealias StartTask = @MainActor (String, TaskOrigin, UInt64) -> TaskCenter.Refusal?
    /// Stops the running task.
    public typealias StopTask = @MainActor () -> Void

    /// What the core says about the exchange.
    public private(set) var status: VoiceStatus
    /// The transcript row, if one is shown.
    public private(set) var transcript: Transcript?
    /// On-device recognition can run here (else voice is off: nothing listens).
    public private(set) var isAvailable = false
    /// The last problem worth showing (activation failed, the core refused a step).
    public private(set) var lastError: String?
    /// The settings the owner chose.
    public private(set) var settings: VoiceSettings
    /// The lines said, in order (the last 50), with their kind.
    public private(set) var spoken: [(text: String, kind: VoiceUtteranceKind)] = []
    /// Final transcripts that reached the core while it said a question was pending (the
    /// UI test's evidence that a spoken "yes" arrived and changed nothing).
    public private(set) var heardWhileQuestionPending = 0
    /// Called after every change.
    public var onChange: (@MainActor () -> Void)?

    private let ports: VoicePorts
    private let center: TaskCenter
    private let clock: any MonotonicClock
    private let tickInterval: Duration?
    private let startTask: StartTask
    private let stopTask: StopTask
    private var core: JarvisVoice
    private var poisoned = false

    private var inbox: [VoiceEvent] = []
    private var working = false
    private var consumers: [Task<Void, Never>] = []
    private var ticker: Task<Void, Never>?
    private var observer: UUID?
    private var running = false

    private var recognitionId: UInt64 = 0
    private var recognizing = false
    private var armId: UInt64 = 0
    private var armed = false
    /// Bumped by `stop()` (which replaces the core): effects of a step from before it are not
    /// carried out, and request ids of the old core are not passed to the new one.
    private var generation: UInt64 = 0
    /// What a session's `voiceRequest` stands for: the core (by generation) and its request
    /// id. Tokens are never reused in a service's lifetime, unlike request ids across cores.
    private var tokens: [UInt64: (generation: UInt64, request: UInt64)] = [:]
    private var nextToken: UInt64 = 1
    /// The phone's lock and Jarvis's place in front, as last reported (kept while stopped,
    /// told to the core on every `start()`).
    private var locked = false
    private var inFront = true

    /// `tickInterval` nil: no timer (tests call `tick()`).
    public init(ports: VoicePorts, center: TaskCenter, settings: VoiceSettings? = nil,
                clock: (any MonotonicClock)? = nil, tickInterval: Duration? = .milliseconds(200),
                startTask: StartTask? = nil, stopTask: StopTask? = nil) {
        let settings = settings ?? defaultVoiceSettings()
        self.ports = ports
        self.center = center
        self.settings = settings
        self.clock = clock ?? SystemMonotonicClock()
        self.tickInterval = tickInterval
        self.startTask = startTask ?? { [center] text, origin, request in
            switch center.start(text, origin: origin, voiceRequest: request) {
            case .success: nil
            case .failure(let refusal): refusal
            }
        }
        self.stopTask = stopTask ?? { [center] in center.cancelSession() }
        // Voice stays off until `start()` has learned that recognition is available.
        var off = settings
        off.wakeWord = false
        core = JarvisVoice(settings: off)
        status = VoiceStatus(phase: .idle, wakeArmed: false, recognizing: false, speaking: false,
                             otherTaskRunning: false, interrupted: false)
    }

    /// Nothing is waiting to be stepped or carried out (tests wait for it).
    public var isSettled: Bool { !working && inbox.isEmpty }

    /// Push to talk can be offered.
    public var canPushToTalk: Bool { isAvailable && !poisoned }

    // MARK: - Lifecycle

    /// Starts listening to the ports and the task center, learns whether recognition is
    /// available, and hands the core the owner's settings (arming the wake word when it is
    /// on).
    public func start() async {
        guard !running else { return }
        running = true
        consume(ports.recognizer.events()) { $0.recognized($1) }
        consume(ports.wakeWord.events()) { $0.wake($1) }
        consume(ports.synthesizer.events()) { service, id in service.send(.speechFinished(utteranceId: id)) }
        consume(ports.audio.events()) { $0.audio($1) }
        observer = center.observe { [weak self] event in self?.taskEvent(event) }
        // The core starts unlocked and in front: tell it what the app reported (before
        // anything can be armed).
        send(.lockChanged(locked: locked))
        send(.foregroundChanged(active: inFront))
        if center.session != nil || center.workflow != nil {
            // A task that runs already is one started elsewhere (this core issued no
            // request yet).
            send(.task(event: .started))
        }
        isAvailable = await ports.recognizer.isAvailable()
        guard running else { return }
        send(.settingsChanged(settings: effective(settings)))
        if let tickInterval {
            ticker = Task { @MainActor [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: tickInterval)
                    if Task.isCancelled { return }
                    self?.tick()
                }
            }
        }
        changed()
    }

    /// Stops everything: nothing listens or speaks afterwards, no event is stepped and no
    /// port is driven until `start()`. The core starts over (a fresh `JarvisVoice` that
    /// knows nothing is armed, listening or speaking), so a later `start()` arms the wake
    /// word again; a task still running then is "started elsewhere" to it (its sessions'
    /// tokens belong to the old core and are reported without a request id).
    public func stop() {
        guard running else { return }
        running = false
        generation &+= 1
        inbox.removeAll()
        ticker?.cancel()
        ticker = nil
        for consumer in consumers { consumer.cancel() }
        consumers = []
        if let observer { center.stopObserving(observer) }
        observer = nil
        if !poisoned {
            var off = settings
            off.wakeWord = false
            core = JarvisVoice(settings: off)
        }
        status = VoiceStatus(phase: .idle, wakeArmed: false, recognizing: false, speaking: false,
                             otherTaskRunning: false, interrupted: false)
        let ports = ports
        let (recognition, arming, wasRecognizing, wasArmed) = (recognitionId, armId, recognizing, armed)
        recognizing = false
        armed = false
        Task {
            if wasRecognizing { await ports.recognizer.stop(id: recognition) }
            if wasArmed { await ports.wakeWord.disarm(id: arming) }
            await ports.synthesizer.stopSpeaking()
            await ports.audio.deactivate()
        }
        transcript = nil
        changed()
    }

    private func consume<Element: Sendable>(_ stream: AsyncStream<Element>,
                                            _ handle: @escaping @MainActor (VoiceService, Element) -> Void) {
        consumers.append(Task { @MainActor [weak self] in
            for await element in stream {
                guard let self else { return }
                handle(self, element)
            }
        })
    }

    // MARK: - The owner

    /// The mic button: pressed starts listening, released ends the request.
    public func pushToTalk(pressed: Bool) {
        guard canPushToTalk || !pressed else { return }
        send(.pushToTalk(pressed: pressed))
    }

    /// Start on the transcript row, with the text as it stands (possibly edited).
    public func confirmTranscript(_ text: String) {
        send(.transcriptConfirmed(text: text))
    }

    /// The transcript row dismissed.
    public func discardTranscript() {
        send(.transcriptDiscarded)
    }

    /// The owner changed the voice settings.
    public func setSettings(_ settings: VoiceSettings) {
        self.settings = settings
        send(.settingsChanged(settings: effective(settings)))
        changed()
    }

    /// The phone locked or unlocked. Kept while the service is stopped and told to the core
    /// on `start()` (the app reports the lock state at launch, before starting).
    public func setLocked(_ locked: Bool) {
        self.locked = locked
        send(.lockChanged(locked: locked))
    }

    /// Jarvis came to the front or left it (the app: in the background or not). Kept while
    /// the service is stopped, like the lock.
    public func setForeground(_ active: Bool) {
        inFront = active
        send(.foregroundChanged(active: active))
    }

    /// Time passed (the timer calls it; tests call it after moving their clock).
    public func tick() {
        send(.tick)
    }

    private func effective(_ settings: VoiceSettings) -> VoiceSettings {
        var s = settings
        if !isAvailable { s.wakeWord = false }
        return s
    }

    // MARK: - Port events

    private func recognized(_ event: SpeechRecognitionEvent) {
        switch event {
        case .partial(let id, let text):
            guard recognizing, id == recognitionId else { return }
            send(.partialTranscript(text: text))
        case .final(let id, let text):
            guard recognizing, id == recognitionId else { return }
            recognizing = false
            sendFinal(text)
        case .failed(let id, let reason):
            guard recognizing, id == recognitionId else { return }
            recognizing = false
            send(.recognizerFailed(reason: reason))
        }
    }

    private func wake(_ event: WakeWordEvent) {
        switch event {
        case .detected(let id):
            guard armed, id == armId else { return }
            send(.wakeDetected)
        case .partial(let id, let text):
            guard armed, id == armId else { return }
            send(.partialTranscript(text: text))
        case .final(let id, let text):
            guard armed, id == armId else { return }
            sendFinal(text)
        case .failed(let id, let reason):
            guard armed, id == armId else { return }
            armed = false
            send(.recognizerFailed(reason: reason))
        }
    }

    private func sendFinal(_ text: String) {
        if case .taskRunning(questionPending: true) = status.phase { heardWhileQuestionPending += 1 }
        send(.finalTranscript(text: text))
    }

    private func audio(_ event: AudioSessionEvent) {
        switch event {
        case .interruptionBegan: send(.audioInterrupted(began: true, shouldResume: false))
        case .interruptionEnded(let shouldResume): send(.audioInterrupted(began: false, shouldResume: shouldResume))
        case .routeChanged: send(.routeChanged)
        }
    }

    // MARK: - Task events

    private func taskEvent(_ event: TaskCenter.Event) {
        switch event {
        case .sessionStarted(let session):
            // A workflow's steps are one task to voice: its start and end bracket them.
            guard session.origin.workflow == nil else { return }
            report(session, .started)
        case .session(let session, .update(let update)):
            guard session.origin.workflow == nil else { return }
            switch update {
            case .confirmationPending, .questionPending:
                report(session, .waitingForApproval)
            case .effectCompleted(.confirm, _), .effectCompleted(.askUser, _):
                report(session, .approvalAnswered)
            default:
                break
            }
        case .session:
            break
        case .sessionFinished(let session, let outcome):
            guard session.origin.workflow == nil else { return }
            report(session, Self.taskEnd(outcome.outcome))
        case .workflowStarted:
            send(.task(event: .started))
        case .workflowFinished(_, let outcome):
            switch outcome.status {
            case .succeeded: send(.task(event: .finishedOk(answer: nil)))
            case .stopped, .cancelled: send(.task(event: .stopped))
            default: send(.task(event: .failed))
            }
        }
    }

    /// A session's event, with the core's request id when the session was started for one
    /// (only then can the core take it for the voice task).
    private func report(_ session: TaskCenter.Session, _ event: VoiceTaskEvent) {
        let token = session.voiceRequest.flatMap { tokens[$0] }
        switch event {
        case .finishedOk, .failed, .stopped:
            if let id = session.voiceRequest { tokens[id] = nil }
        default:
            break
        }
        if let token, token.generation == generation {
            send(.requestTask(request: token.request, event: event))
        } else {
            send(.task(event: event))
        }
    }

    /// The core's outcome as a voice task event; the summary is the answer.
    static func taskEnd(_ outcome: Outcome) -> VoiceTaskEvent {
        switch outcome.status {
        case .success: .finishedOk(answer: outcome.summary.isEmpty ? nil : outcome.summary)
        case .cancelled: .stopped
        case .failed, .blocked, .interrupted: .failed
        }
    }

    // MARK: - The inbox

    private func send(_ event: VoiceEvent) {
        // Stopped: nothing is stepped (and so no port is driven) until `start()`.
        guard !poisoned, running else { return }
        inbox.append(event)
        guard !working else { return }
        working = true
        Task { @MainActor [weak self] in await self?.drain() }
    }

    private func drain() async {
        while !inbox.isEmpty {
            let generation = self.generation
            let event = inbox.removeFirst()
            let effects: [VoiceEffect]
            do {
                effects = try core.step(nowMs: clock.nowMs(), event: event)
            } catch {
                await poison(error)
                break
            }
            for effect in effects {
                // `stop()` while this step's effects were carried out: the rest are dropped.
                guard running, generation == self.generation else { break }
                await perform(effect)
            }
            guard generation == self.generation else { continue }
            if let now = try? core.status() { status = now }
            // The row lives as long as its exchange: a row waiting for a touch only while the
            // core is Confirming, any row only while an exchange is open.
            if case .confirming = status.phase {
            } else if transcript?.needsConfirm == true || status.phase == .idle {
                transcript = nil
            }
            changed()
        }
        working = false
    }

    private func perform(_ effect: VoiceEffect) async {
        switch effect {
        case .armWakeWord:
            armId &+= 1
            armed = true
            await ports.wakeWord.arm(id: armId)
        case .disarmWakeWord:
            armed = false
            await ports.wakeWord.disarm(id: armId)
        case .startRecognizer:
            recognitionId &+= 1
            recognizing = true
            await ports.recognizer.start(id: recognitionId)
        case .finishRecognizer:
            await ports.recognizer.finish(id: recognitionId)
        case .stopRecognizer:
            recognizing = false
            await ports.recognizer.stop(id: recognitionId)
        case .playCue(let cue):
            await ports.synthesizer.playCue(cue == .listening ? .listening : .ended)
        case .speak(let id, let text, let kind):
            spoken.append((text, kind))
            if spoken.count > 50 { spoken.removeFirst(spoken.count - 50) }
            await ports.synthesizer.speak(text, id: id)
        case .stopSpeaking:
            await ports.synthesizer.stopSpeaking()
        case .showTranscript(let text, let isFinal, let needsConfirm, let endpointed):
            transcript = Transcript(text: text, isFinal: isFinal, needsConfirm: needsConfirm, endpointed: endpointed)
        case .clearTranscript:
            transcript = nil
        case .startTask(let text, let origin, let request):
            // The origin is the core's: voice without a touch, the owner's words only for a
            // transcript confirmed by touch. The request id goes with the session, so its
            // events (and this refusal) say which start they belong to.
            let token = nextToken
            nextToken &+= 1
            tokens[token] = (generation, request)
            let refusal = startTask(text, origin == .phone ? .owner : .voice, token)
            if let refusal {
                tokens[token] = nil
                let busy = if case .busy = refusal { true } else if case .workflowRunning = refusal { true } else { false }
                send(.requestTask(request: request, event: .startRefused(busy: busy)))
            }
        case .stopTask:
            stopTask()
        case .activateAudioSession:
            do {
                try await ports.audio.activate()
            } catch {
                lastError = "The audio session could not start (\(error)). Voice is paused."
                // As an interruption that ended without "resume": nothing listens until the
                // owner acts again (the mic button, the settings, coming back to Jarvis).
                send(.audioInterrupted(began: true, shouldResume: false))
                send(.audioInterrupted(began: false, shouldResume: false))
            }
        case .deactivateAudioSession:
            await ports.audio.deactivate()
        }
    }

    /// The core caught a panic: voice is off for good in this run.
    private func poison(_ error: any Error) async {
        poisoned = true
        inbox.removeAll()
        lastError = "Voice stopped after an internal error (\(CoreErrorMessage(error).message))."
        if recognizing { await ports.recognizer.stop(id: recognitionId) }
        if armed { await ports.wakeWord.disarm(id: armId) }
        recognizing = false
        armed = false
        await ports.synthesizer.stopSpeaking()
        await ports.audio.deactivate()
        transcript = nil
    }

    private func changed() {
        onChange?()
    }
}
