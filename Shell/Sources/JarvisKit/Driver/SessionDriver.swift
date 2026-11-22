import Foundation
import JarvisCore

/// The IO the driver performs on the core's behalf.
public struct DriverDependencies: Sendable {
    /// The phone actuator.
    public var runner: any PhoneRunner
    /// The model backend.
    public var model: any ModelProvider
    /// Native tools.
    public var tools: any ToolHost
    /// The owner (confirmations, questions).
    public var human: any HumanGate
    /// Where the memory JSON goes at the end of the run.
    public var memory: any MemoryPersisting
    /// Where screenshots are kept for the model (share it with the model backend).
    public var screenshots: ScreenshotStore
    /// Reaches the owner when the phone must be unlocked.
    public var attention: any OwnerAttention
    /// Timestamps for the core's events.
    public var clock: any MonotonicClock
    /// Where the Live view's frames go when `DriverConfiguration.viewerScreenshots` is on
    /// (keyed by `ViewerFrameKey(taskId:observation:)`); `nil` keeps none.
    public var viewerFrames: ViewerFrameStore?

    /// Bundles the dependencies; the clock defaults to the system's monotonic clock.
    public init(
        runner: any PhoneRunner,
        model: any ModelProvider,
        tools: any ToolHost,
        human: any HumanGate,
        memory: any MemoryPersisting,
        screenshots: ScreenshotStore,
        attention: any OwnerAttention = NoOwnerAttention(),
        clock: any MonotonicClock = SystemMonotonicClock(),
        viewerFrames: ViewerFrameStore? = nil
    ) {
        self.runner = runner
        self.model = model
        self.tools = tools
        self.human = human
        self.memory = memory
        self.screenshots = screenshots
        self.attention = attention
        self.clock = clock
        self.viewerFrames = viewerFrames
    }
}

/// Runs one Jarvis session: feeds the core's effects to the IO dependencies and their
/// results back to the core as events until the core reports `Effect.finished`.
///
/// - Every IO failure becomes the matching failure event (`observeFailed` after the
///   observe retries, `modelFailed`, a failed `actResult` / `toolResult`, a declined
///   `userDecision`); see the README's table. Nothing throws out of `run()`.
/// - Locked phone (docs/ARCHITECTURE.md, *Locked phone*): a locked snapshot goes to the core
///   as a locked observation and a locked refusal as `RunnerFailure.deviceLocked`; a lock
///   noticed while waiting for the model, a tool or the owner (`lockWatchInterval`) abandons
///   that wait with `Event.deviceLocked`. `Effect.awaitUnlock` tells `OwnerAttention` once
///   per wait, then polls `PhoneRunner.lockState()` every `unlockPollInterval` and answers
///   `tick` (still locked), `deviceUnlocked`, or a fresh observation when the runner cannot
///   tell. Cancellation works throughout.
/// - Each IO has a deadline (`DriverConfiguration`).
/// - `cancel()`, or cancelling the task that runs `run()`, delivers `Event.cancel` promptly,
///   also while an IO is in flight (the IO is abandoned).
/// - The memory JSON from `JarvisSession.finish()` is handed to `MemoryPersisting.save`
///   exactly once, on every exit path.
/// - `run()` may be called more than once and concurrently: the session runs once, and
///   every call returns that run's result.
///
/// `JarvisSession.step` is synchronous and fast, so it is called directly on the actor.
public actor SessionDriver {
    private enum Phase {
        case idle
        case running
        case finished(RunResult)
    }

    /// Progress, effects, results, pending confirmations and the final result of the run.
    /// Ends after `.finished`. Single consumer (an `AsyncStream`).
    public nonisolated let updates: AsyncStream<DriverUpdate>

    private let session: JarvisSession
    private let task: TaskSpec
    private let deps: DriverDependencies
    private let configuration: DriverConfiguration
    private let continuation: AsyncStream<DriverUpdate>.Continuation

    private var phase = Phase.idle
    private var waiters: [CheckedContinuation<RunResult, Never>] = []
    private var cancelRequested = false
    private var stopReason: EndReason?
    private var cancelInFlightIO: (@Sendable () -> Void)?
    private var lastObservationIssue: String?
    private var waitingForUnlock = false
    /// The number of the effect being carried out (names the Live view's frames).
    private var currentEffectNumber = 0

    /// Creates the session. Throws when the core rejects `memoryJSON` (corrupt or from an
    /// incompatible core); quarantine that memory and retry with `""` (fresh memory).
    ///
    /// Build `config` from the core's defaults (`SessionConfigBuilder`), never with the
    /// generated memberwise initialiser.
    public init(
        config: SessionConfig,
        task: TaskSpec,
        memoryJSON: String,
        dependencies: DriverDependencies,
        configuration: DriverConfiguration = DriverConfiguration()
    ) throws {
        self.session = try JarvisSession(config: config, task: task, memoryJson: memoryJSON)
        self.task = task
        self.deps = dependencies
        self.configuration = configuration
        let (stream, continuation) = AsyncStream.makeStream(
            of: DriverUpdate.self,
            bufferingPolicy: .bufferingNewest(max(1, configuration.updateBufferSize))
        )
        self.updates = stream
        self.continuation = continuation
    }

    /// Runs the session to the end and returns how it ended. Never throws; IO errors and
    /// cancellation are part of the result.
    public func run() async -> RunResult {
        switch phase {
        case .finished(let result):
            return result
        case .running:
            return await withCheckedContinuation { waiters.append($0) }
        case .idle:
            phase = .running
        }
        let result = await drive()
        phase = .finished(result)
        let waiting = waiters
        waiters = []
        waiting.forEach { $0.resume(returning: result) }
        return result
    }

    /// Stops the run: `Event.cancel` goes to the core as soon as possible (an IO in flight
    /// is abandoned). Before `run()` it makes the run end immediately as cancelled.
    public func cancel() {
        cancelRequested = true
        cancelInFlightIO?()
    }

    // MARK: - The loop

    private func drive() async -> RunResult {
        emit(.started(task))
        var event = Event.start
        var effectCount = 0
        var modelCalls = 0
        var outcome: Outcome?
        while outcome == nil {
            if stopReason == nil, cancelRequested || Task.isCancelled {
                stopReason = .cancelled
            }
            if stopReason != nil {
                event = .cancel
            }
            let deliveredCancel = event == .cancel
            let parsedRunnerText = if case .observedRunnerText = event { true } else { false }
            let effect = session.step(nowMs: deps.clock.nowMs(), event: event)
            effectCount += 1
            currentEffectNumber = effectCount
            if parsedRunnerText { reportObservationIssue() }
            emit(.effect(effect, number: effectCount))
            if let progress = try? session.progress() {
                emit(.progress(progress))
            }
            if case .awaitUnlock = effect {} else { await endUnlockWait() }
            if case .finished(let finished) = effect {
                outcome = finished
                break
            }
            if deliveredCancel {
                // The core honours Cancel in every state; this is a backstop only.
                outcome = Outcome(
                    status: .cancelled,
                    summary: "The core did not finish after Cancel.",
                    steps: (try? session.progress().step) ?? 0,
                    durationMs: 0
                )
                break
            }
            if effectCount >= configuration.maxEffects {
                stopReason = .effectLimitReached(effectCount)
                continue
            }
            if case .infer = effect { modelCalls += 1 }
            let next = await perform(effect)
            emit(.effectCompleted(effect, event: next))
            event = next
        }
        await endUnlockWait()
        let memory = await persistMemory()
        let result = RunResult(
            outcome: outcome ?? Outcome(status: .failed, summary: "No outcome.", steps: 0, durationMs: 0),
            endReason: stopReason ?? .sessionFinished,
            memory: memory,
            effectCount: effectCount,
            modelCalls: modelCalls
        )
        emit(.finished(result))
        continuation.finish()
        return result
    }

    /// Carries out one effect and returns the event for the core. Never throws.
    private func perform(_ effect: Effect) async -> Event {
        switch effect {
        case .observe(let screenshot):
            return await observe(screenshot: screenshot)
        case .infer(let request):
            return await infer(request)
        case .act(let command):
            return await act(command)
        case .callTool(let call):
            return await callTool(call)
        case .confirm(let request):
            return await confirm(request)
        case .askUser(let question):
            return await ask(question)
        case .awaitUnlock(let reason, let waitedMs):
            return await awaitUnlock(reason: reason, waitedMs: waitedMs)
        case .finished:
            return stop(.cancelled) // unreachable: the loop ends on .finished
        }
    }

    private func observe(screenshot: Bool) async -> Event {
        let runner = deps.runner
        let attempts = max(1, configuration.observeAttempts)
        // The Live view's frames: pixels on every observation, without changing what the
        // core is given (below).
        let viewerFrames = configuration.viewerScreenshots ? deps.viewerFrames : nil
        let forViewerOnly = viewerFrames != nil && !screenshot
        let requestPixels = screenshot || viewerFrames != nil
        var lastError = "no attempt made"
        for attempt in 1...attempts {
            do {
                let observation = try await io(.observe, configuration.observeTimeout) {
                    try await runner.snapshot(includeScreenshot: requestPixels)
                }
                for note in observation.notes { emit(.note(note)) }
                if let viewerFrames, let frame = observation.screenshot, !observation.deviceLocked {
                    viewerFrames.put(frame, for: ViewerFrameKey(taskId: task.id, observation: currentEffectNumber))
                }
                // Pixels taken only for the viewer never reach the core or its store.
                let ref = forViewerOnly ? nil : observation.screenshot.map { deps.screenshots.store($0) }
                return observation.event(screenshot: ref)
            } catch let error where isOurCancellation(error) {
                return stop(.cancelled)
            } catch {
                lastError = describe(error)
                let retry = attempt < attempts
                emit(.ioFailed(.observe, message: lastError, willRetry: retry))
                guard retry else { break }
                let delay = configuration.observeRetryDelay
                do {
                    try await io(.observe, nil) { try await Task.sleep(for: delay) }
                } catch {
                    if isOurCancellation(error) { return stop(.cancelled) }
                }
            }
        }
        // The core counts unreadable screens itself (`max_consecutive_failures`) and
        // finishes Failed; while it waits for an unlock it does not count them.
        return .observeFailed(reason: lastError)
    }

    private func infer(_ request: ModelRequest) async -> Event {
        let model = deps.model
        do {
            let watched = try await ioWatchingLock(.infer, configuration.inferTimeout) {
                try await model.generate(request)
            }
            guard case .value(let text) = watched else { return lockDetected(.infer) }
            return .modelOutput(output: ModelOutput(text: text))
        } catch let error where isOurCancellation(error) {
            return stop(.cancelled)
        } catch {
            let message = describe(error)
            emit(.ioFailed(.infer, message: message, willRetry: false))
            return .modelFailed(error: message)
        }
    }

    private func act(_ command: RunnerCommand) async -> Event {
        let runner = deps.runner
        do {
            let result = try await io(.act, configuration.actTimeout) {
                try await runner.perform(command)
            }
            return .actResult(result: result)
        } catch let error where isOurCancellation(error) {
            return stop(.cancelled)
        } catch {
            let message = describe(error)
            emit(.ioFailed(.act, message: message, willRetry: false))
            return .actResult(result: RunnerResult(ok: false, error: message))
        }
    }

    private func callTool(_ call: ToolCall) async -> Event {
        let tools = deps.tools
        do {
            let watched = try await ioWatchingLock(.tool, configuration.toolTimeout) {
                await tools.execute(call)
            }
            guard case .value(let result) = watched else { return lockDetected(.tool) }
            return .toolResult(result: result)
        } catch let error where isOurCancellation(error) {
            return stop(.cancelled)
        } catch {
            let message = describe(error)
            emit(.ioFailed(.tool, message: message, willRetry: false))
            return .toolResult(result: ToolResult(ok: false, output: "", error: message))
        }
    }

    private func confirm(_ request: ConfirmRequest) async -> Event {
        emit(.confirmationPending(request))
        let human = deps.human
        do {
            let watched = try await ioWatchingLock(.confirm, configuration.humanTimeout) {
                try await human.confirm(request)
            }
            // A lock while the owner decides: the core asks again after the unlock.
            guard case .value(let approved) = watched else { return lockDetected(.confirm) }
            return .userDecision(approved: approved)
        } catch let error where isOurCancellation(error) {
            return stop(.cancelled)
        } catch {
            // No decision is a decline: a risky action never runs without a yes.
            emit(.ioFailed(.confirm, message: describe(error), willRetry: false))
            return .userDecision(approved: false)
        }
    }

    private func ask(_ question: String) async -> Event {
        emit(.questionPending(question))
        let human = deps.human
        do {
            let watched = try await ioWatchingLock(.answer, configuration.humanTimeout) {
                try await human.answer(question)
            }
            guard case .value(let answer) = watched else { return lockDetected(.answer) }
            return .userAnswer(text: answer)
        } catch let error where isOurCancellation(error) {
            return stop(.cancelled)
        } catch {
            // An invented or empty answer could mislead the model; stop instead.
            let message = describe(error)
            emit(.ioFailed(.answer, message: message, willRetry: false))
            return stop(.noAnswer(message))
        }
    }

    // MARK: - Locked phone

    private func awaitUnlock(reason: String, waitedMs: UInt64) async -> Event {
        emit(.awaitingUnlock(reason: reason, waitedMs: waitedMs))
        if !waitingForUnlock {
            waitingForUnlock = true
            await tellOwner { await $0.awaitUnlock(reason: reason, waitedMs: waitedMs) }
        }
        let delay = configuration.unlockPollInterval
        do {
            try await io(.awaitUnlock, nil) { try await Task.sleep(for: delay) }
        } catch {
            if isOurCancellation(error) { return stop(.cancelled) }
        }
        let runner = deps.runner
        let state: DeviceLockState
        do {
            state = try await io(.lockState, configuration.observeTimeout) { try await runner.lockState() }
        } catch let error where isOurCancellation(error) {
            return stop(.cancelled)
        } catch {
            emit(.ioFailed(.lockState, message: describe(error), willRetry: false))
            state = .unknown
        }
        switch state {
        case .locked: return .tick
        case .unlocked: return .deviceUnlocked
        // The runner cannot tell: a fresh observation answers (locked: keep waiting).
        case .unknown: return await observe(screenshot: false)
        }
    }

    private func endUnlockWait() async {
        guard waitingForUnlock else { return }
        waitingForUnlock = false
        emit(.unlockWaitEnded)
        await tellOwner { await $0.unlockWaitEnded() }
    }

    /// Calls `OwnerAttention` with a short deadline, outside the run's cancellation (so a
    /// cancelled run still clears its "unlock to continue" notice).
    private func tellOwner(_ call: @escaping @Sendable (any OwnerAttention) async -> Void) async {
        let attention = deps.attention
        await Task.detached {
            try? await IORace<Void>().run(kind: .attention, timeout: .seconds(5)) { await call(attention) }
        }.value
    }

    private func lockDetected(_ kind: IOKind) -> Event {
        emit(.lockDetected(kind))
        return .deviceLocked
    }

    private enum Watched<T: Sendable>: Sendable {
        case value(T)
        case locked
    }

    /// `io`, raced against the runner reporting a lock (polled every
    /// `lockWatchInterval`): the wait for the model, a tool or the owner is abandoned when
    /// the phone locks, and the core gets `Event.deviceLocked` in its place.
    private func ioWatchingLock<T: Sendable>(
        _ kind: IOKind,
        _ timeout: Duration?,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> Watched<T> {
        guard let interval = configuration.lockWatchInterval else {
            return .value(try await io(kind, timeout, operation))
        }
        if cancelRequested || Task.isCancelled { throw CancellationError() }
        let race = IORace<Watched<T>>()
        cancelInFlightIO = { race.cancel() }
        defer { cancelInFlightIO = nil }
        let runner = deps.runner
        let watcher = Task {
            while !Task.isCancelled {
                do { try await Task.sleep(for: interval) } catch { return }
                if (try? await runner.lockState()) == .locked {
                    race.resolve(.locked)
                    return
                }
            }
        }
        defer { watcher.cancel() }
        return try await race.run(kind: kind, timeout: timeout) { .value(try await operation()) }
    }

    // MARK: - Helpers

    /// Runs `operation` with a deadline, abandonable by `cancel()` and task cancellation.
    private func io<T: Sendable>(
        _ kind: IOKind,
        _ timeout: Duration?,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        if cancelRequested || Task.isCancelled { throw CancellationError() }
        let race = IORace<T>()
        cancelInFlightIO = { race.cancel() }
        defer { cancelInFlightIO = nil }
        return try await race.run(kind: kind, timeout: timeout, operation)
    }

    /// A `CancellationError` caused by `cancel()` or task cancellation (an IO that throws
    /// `CancellationError` on its own is an ordinary failure).
    private func isOurCancellation(_ error: any Error) -> Bool {
        error is CancellationError && (cancelRequested || Task.isCancelled)
    }

    /// Records why the driver stops the session and returns the event that stops it.
    private func stop(_ reason: EndReason) -> Event {
        if stopReason == nil { stopReason = reason }
        return .cancel
    }

    private func reportObservationIssue() {
        guard let issue = (try? session.lastObservationIssue()) ?? nil, issue != lastObservationIssue else {
            return
        }
        lastObservationIssue = issue
        emit(.note(issue))
    }

    /// `finish()` once, then one `save`, in a detached task so a cancelled run still saves.
    private func persistMemory() async -> MemorySaveStatus {
        let json: String
        do {
            json = try session.finish()
        } catch {
            return .unavailable(describe(error))
        }
        let persister = deps.memory
        let timeout = configuration.memorySaveTimeout
        let save = Task.detached { () -> MemorySaveStatus in
            do {
                try await IORace<Void>().run(kind: .saveMemory, timeout: timeout) {
                    try await persister.save(json)
                }
                return .saved
            } catch {
                return .saveFailed(String(describing: error))
            }
        }
        let status = await save.value
        if case .saveFailed(let message) = status {
            emit(.ioFailed(.saveMemory, message: message, willRetry: false))
        }
        return status
    }

    private func emit(_ update: DriverUpdate) {
        continuation.yield(update)
    }

    private func describe(_ error: any Error) -> String {
        String(describing: error)
    }
}
