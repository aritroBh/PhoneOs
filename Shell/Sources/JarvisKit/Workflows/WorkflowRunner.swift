import Foundation
import JarvisCore

/// Posts a workflow's Notify step. The app's adapter uses local notifications; failing to
/// post does not stop the run (the step's text is also shown in the run view).
public protocol WorkflowNotifier: Sendable {
    func post(_ text: String, workflow: String) async throws
}

/// Drops notifications (tests, previews).
public struct SilentWorkflowNotifier: WorkflowNotifier {
    public init() {}
    public func post(_ text: String, workflow: String) async throws {}
}

/// Drives one `JarvisWorkflowRun` at a time to its end:
///
/// | Effect | What the shell does | Event back |
/// |---|---|---|
/// | `startSession(task, skill)` | `TaskCenter.run` (same entry, policy and gate as Ask) | `sessionFinished(status, summary)` |
/// | `askOwner(question)` | the broker's yes/no prompt | `ownerAnswered(yes:)` (withdrawn: no) |
/// | `notify(text)` | `WorkflowNotifier.post` | `notified` |
/// | `wait(untilMs)` | sleeps on the monotonic clock, cancellable | `waitElapsed` |
/// | `finished(outcome)` | `recordRun`, an Activity entry grouping the sessions | (none) |
///
/// `cancel()` (or cancelling the task `start` returned) stops the current session, withdraws
/// a question, ends a wait and feeds `cancel` to the run.
@MainActor
public final class WorkflowRunner {
    public enum StepState: Sendable, Equatable {
        case pending, running, done, failed
        /// A "only continue if" step that did not hold: the run stopped here.
        case stoppedHere
    }

    /// What the run is doing now.
    public enum Activity: Sendable, Equatable {
        case starting
        case session(String)
        case asking(String)
        case notifying(String)
        case waiting(untilMs: UInt64)
        case finished
    }

    /// The current run, or the last one.
    public struct Progress: Sendable, Equatable {
        public let runId: String
        public let workflow: Workflow
        public let origin: TaskOrigin
        public var current: Int?
        public var states: [StepState]
        public var activity: Activity
        public var outcome: WorkflowOutcome?
        /// History ids of the sessions the run started, in order.
        public var sessionIds: [String]
        /// Notify texts posted, in order (also shown in the run view).
        public var notes: [String]

        public var isRunning: Bool { outcome == nil }
    }

    /// Why `start` refused.
    public enum StartRefusal: Error, Sendable, Equatable, CustomStringConvertible {
        case alreadyRunning(String)
        case busy(String)
        case notFound(String)
        case core(String)

        public var description: String {
            switch self {
            case .alreadyRunning(let name): "The workflow \u{201C}\(name)\u{201D} is already running; one workflow runs at a time."
            case .busy(let why): why
            case .notFound(let key): "There is no workflow \u{201C}\(key)\u{201D}."
            case .core(let message): message
            }
        }
    }

    public let library: WorkflowLibrary
    public let center: TaskCenter
    private let notifier: any WorkflowNotifier
    private let clock: any MonotonicClock
    private let limits: RunLimits?
    private let sleep: @Sendable (Duration) async throws -> Void
    private let wallClock: @Sendable () -> Date

    public private(set) var progress: Progress?
    private var task: Task<WorkflowOutcome, Never>?
    /// Called after every change of `progress`.
    public var onChange: (@MainActor () -> Void)?

    public init(library: WorkflowLibrary, center: TaskCenter, notifier: any WorkflowNotifier = SilentWorkflowNotifier(),
                clock: any MonotonicClock = SystemMonotonicClock(), limits: RunLimits? = nil,
                sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) },
                wallClock: @escaping @Sendable () -> Date = Date.init) {
        self.library = library
        self.center = center
        self.notifier = notifier
        self.clock = clock
        self.limits = limits
        self.sleep = sleep
        self.wallClock = wallClock
    }

    public var isRunning: Bool { task != nil }

    /// Starts workflow `id`. Refused while a workflow or a task runs.
    public func start(id: String, origin: TaskOrigin = .owner) -> Result<Task<WorkflowOutcome, Never>, StartRefusal> {
        if task != nil, let running = progress {
            return .failure(.alreadyRunning(running.workflow.name))
        }
        guard let workflow = library.workflow(id: id) else { return .failure(.notFound(id)) }
        let run: JarvisWorkflowRun
        do {
            run = try library.makeRun(id: id, limits: limits ?? defaultRunLimits())
        } catch {
            return .failure(.core(String(describing: error)))
        }
        let slot = TaskCenter.WorkflowSlot(runId: UUID().uuidString, workflowId: workflow.id, name: workflow.name, origin: origin)
        if let refusal = center.beginWorkflow(slot) {
            if case .workflowRunning(let name) = refusal { return .failure(.alreadyRunning(name)) }
            return .failure(.busy(refusal.description))
        }
        progress = Progress(runId: slot.runId, workflow: workflow, origin: origin, current: nil,
                            states: Array(repeating: .pending, count: workflow.steps.count), activity: .starting,
                            outcome: nil, sessionIds: [], notes: [])
        changed()
        let started = clock.nowMs()
        let task = Task { @MainActor [weak self] () -> WorkflowOutcome in
            guard let self else {
                return WorkflowOutcome(status: .cancelled, stepsRun: 0, summary: "Jarvis went away.", failedStep: nil)
            }
            let outcome = await self.drive(run, slot: slot)
            self.finish(slot, outcome: outcome, startedMs: started)
            return outcome
        }
        self.task = task
        return .success(task)
    }

    /// Runs workflow `id` to the end (App Intent, remote requests that wait).
    public func run(id: String, origin: TaskOrigin = .owner) async -> Result<WorkflowOutcome, StartRefusal> {
        switch start(id: id, origin: origin) {
        case .failure(let refusal): return .failure(refusal)
        case .success(let task): return .success(await task.value)
        }
    }

    /// Stops the run: the current session, a question or a wait, then the run itself.
    public func cancel() {
        guard let task else { return }
        task.cancel()
        center.cancelSession()
        center.broker.cancelAll()
    }

    // MARK: - The loop

    private func drive(_ run: JarvisWorkflowRun, slot: TaskCenter.WorkflowSlot) async -> WorkflowOutcome {
        var event: WorkflowEvent = .start
        while true {
            if Task.isCancelled { event = .cancel }
            let effect: WorkflowEffect
            do {
                let nowMs = clock.nowMs()
                let next = event
                effect = try library.skills.withStore { try run.step(nowMs: nowMs, event: next, skills: $0) }
            } catch {
                // A protocol error cannot happen with this loop; a poisoned run ends here.
                let message = CoreErrorMessage(error).message
                return (try? run.finish(nowMs: clock.nowMs()))
                    ?? WorkflowOutcome(status: .failed, stepsRun: 0, summary: "The workflow run failed: \(message)", failedStep: nil)
            }
            moveTo((try? run.currentStep()).flatMap { $0 }.map(Int.init))
            switch effect {
            case .finished(let outcome):
                return outcome
            case .startSession(let text, let skill):
                setActivity(.session(text))
                let tag = WorkflowRunTag(runId: slot.runId, workflowId: slot.workflowId, name: slot.name,
                                         step: progress?.current ?? 0)
                switch await center.run(text, skill: skill, origin: .workflow(tag)) {
                case .failure(let refusal):
                    event = .sessionFinished(status: .failed, summary: refusal.description)
                case .success(let outcome):
                    if case .finished(let spec, _) = outcome { progress?.sessionIds.append(spec.id) }
                    let o = outcome.outcome
                    markCurrent(o.status == .success ? .done : .failed)
                    event = .sessionFinished(status: o.status, summary: o.summary)
                }
            case .askOwner(let question):
                setActivity(.asking(question))
                do {
                    let yes = try await center.broker.askYesNo(question)
                    markCurrent(.done)
                    event = .ownerAnswered(yes: yes, text: yes ? "yes" : "no")
                } catch {
                    // Withdrawn (the run was stopped, or the sheet went away): no yes.
                    event = Task.isCancelled ? .cancel : .ownerAnswered(yes: false, text: "")
                }
            case .notify(let text):
                setActivity(.notifying(text))
                progress?.notes.append(text)
                try? await notifier.post(text, workflow: slot.name)
                markCurrent(.done)
                event = .notified
            case .wait(let untilMs):
                setActivity(.waiting(untilMs: untilMs))
                let now = clock.nowMs()
                do {
                    if untilMs > now { try await sleep(.milliseconds(Int64(clamping: untilMs - now))) }
                    markCurrent(.done)
                    event = .waitElapsed
                } catch {
                    event = .cancel
                }
            }
        }
    }

    private func finish(_ slot: TaskCenter.WorkflowSlot, outcome: WorkflowOutcome, startedMs: UInt64) {
        if var p = progress, p.runId == slot.runId {
            for i in p.states.indices where p.states[i] == .running {
                p.states[i] = outcome.status == .succeeded || outcome.status == .completedWithFailures ? .done : .failed
            }
            if let failed = outcome.failedStep.map(Int.init), p.states.indices.contains(failed) {
                p.states[failed] = .failed
            }
            if outcome.status == .stopped, outcome.stepsRun > 0 {
                let at = Int(outcome.stepsRun) - 1
                if p.states.indices.contains(at) { p.states[at] = .stoppedHere }
            }
            p.outcome = outcome
            p.activity = .finished
            p.current = nil
            progress = p
        }
        try? library.recordRun(id: slot.workflowId, outcome: outcome)
        let record = TaskRecord(
            workflowRun: slot.runId, name: slot.name, outcome: outcome, sessionIds: progress?.sessionIds ?? [],
            durationMs: Int(clamping: clock.nowMs() &- startedMs), requestedBy: slot.origin.remoteDeviceName,
            finishedAt: wallClock()
        )
        _ = try? TaskHistoryStore(fileURL: center.coordinator.paths.history).append(record)
        task = nil
        center.endWorkflow(slot, outcome: outcome)
        changed()
    }

    // MARK: - Progress

    private func moveTo(_ index: Int?) {
        guard var p = progress else { return }
        if let index {
            // Steps the run passed without an effect ("only continue if" that held) are done.
            for i in p.states.indices where i < index && (p.states[i] == .pending || p.states[i] == .running) {
                p.states[i] = .done
            }
            if p.states.indices.contains(index), p.states[index] == .pending { p.states[index] = .running }
        }
        p.current = index
        progress = p
        changed()
    }

    private func markCurrent(_ state: StepState) {
        guard var p = progress, let i = p.current, p.states.indices.contains(i) else { return }
        p.states[i] = state
        progress = p
        changed()
    }

    private func setActivity(_ activity: Activity) {
        progress?.activity = activity
        changed()
    }

    private func changed() { onChange?() }
}
