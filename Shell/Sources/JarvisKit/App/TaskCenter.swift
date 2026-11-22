import Foundation
import JarvisCore

/// The workflow run a session belongs to.
public struct WorkflowRunTag: Sendable, Equatable {
    public var runId: String
    public var workflowId: String
    public var name: String
    /// 0-based index of the step that started the session.
    public var step: Int

    public init(runId: String, workflowId: String, name: String, step: Int) {
        self.runId = runId
        self.workflowId = workflowId
        self.name = name
        self.step = step
    }
}

/// Who asked for a task or a workflow.
public enum TaskOrigin: Sendable, Equatable {
    /// The owner on the phone: Ask, Siri/Shortcuts, Run now, a schedule.
    case owner
    /// A paired device; `taskId` is the hub's id for the request.
    case remote(taskId: UInt64, deviceId: String, deviceName: String?)
    /// A step of a workflow run.
    case workflow(WorkflowRunTag)

    public var workflow: WorkflowRunTag? {
        if case .workflow(let tag) = self { return tag }
        return nil
    }

    /// The hub's task id when a paired device asked.
    public var remoteTaskId: UInt64? {
        if case .remote(let id, _, _) = self { return id }
        return nil
    }

    public var remoteDeviceName: String? {
        if case .remote(_, _, let name) = self { return name }
        return nil
    }
}

/// The one entry every task takes, whoever asks: the owner (Ask, Siri, Run now), a workflow
/// step or a paired device. One session at a time, one workflow at a time; observers (the
/// run view, remote control) see every session's events in order on the main actor.
///
/// A session goes through `JarvisCoordinator.run` with the owner's settings, the one
/// `HumanPromptBroker` and the same `OwnerAttention`, so nothing that starts here can skip
/// a confirmation, the first-use question or the lock handling.
@MainActor
public final class TaskCenter {
    /// A session that is running.
    public struct Session: Sendable, Equatable, Identifiable {
        public let id: UUID
        public let text: String
        public let skill: SkillContext?
        public let origin: TaskOrigin
    }

    /// The workflow run holding the slot.
    public struct WorkflowSlot: Sendable, Equatable {
        public let runId: String
        public let workflowId: String
        public let name: String
        /// Who started it (`.owner` or `.remote`).
        public let origin: TaskOrigin

        public init(runId: String, workflowId: String, name: String, origin: TaskOrigin) {
            self.runId = runId
            self.workflowId = workflowId
            self.name = name
            self.origin = origin
        }
    }

    public enum Event: Sendable {
        case sessionStarted(Session)
        case session(Session, RunEvent)
        case sessionFinished(Session, TaskRunOutcome)
        case workflowStarted(WorkflowSlot)
        case workflowFinished(WorkflowSlot, WorkflowOutcome)
    }

    /// Why a start was refused.
    public enum Refusal: Error, Sendable, Equatable, CustomStringConvertible {
        case emptyTask
        case busy
        case workflowRunning(String)
        /// The owner's rules cannot be used yet (unreadable before the first unlock, or
        /// damaged and not yet reviewed); the text says why in plain words.
        case settingsUnavailable(String)

        public var description: String {
            switch self {
            case .emptyTask: "There is nothing to do."
            case .busy: "Jarvis is already running a task."
            case .workflowRunning(let name): "The workflow \u{201C}\(name)\u{201D} is running."
            case .settingsUnavailable(let why): why
            }
        }
    }

    public let coordinator: JarvisCoordinator
    public let broker: HumanPromptBroker
    private let attention: any OwnerAttention
    private let settings: @MainActor () -> JarvisSettings
    private let settingsProblem: @MainActor () -> String?

    public private(set) var session: Session?
    public private(set) var workflow: WorkflowSlot?
    private var sessionTask: Task<TaskRunOutcome, Never>?
    private var observers: [UUID: @MainActor (Event) -> Void] = [:]
    private var observerOrder: [UUID] = []

    /// `settingsProblem` says why the owner's rules cannot be used (`SettingsGate.refusal`);
    /// while it returns a text, every session and workflow start is refused with it, so no
    /// task runs under defaults because the rules could not be read.
    public init(coordinator: JarvisCoordinator, broker: HumanPromptBroker, attention: any OwnerAttention = NoOwnerAttention(),
                settings: @escaping @MainActor () -> JarvisSettings,
                settingsProblem: @escaping @MainActor () -> String? = { nil }) {
        self.coordinator = coordinator
        self.broker = broker
        self.attention = attention
        self.settings = settings
        self.settingsProblem = settingsProblem
    }

    public var isBusy: Bool { session != nil || workflow != nil }

    /// Registers `handler` for every event, in registration order. Returns a token for
    /// `stopObserving`.
    @discardableResult
    public func observe(_ handler: @escaping @MainActor (Event) -> Void) -> UUID {
        let id = UUID()
        observers[id] = handler
        observerOrder.append(id)
        return id
    }

    public func stopObserving(_ id: UUID) {
        observers[id] = nil
        observerOrder.removeAll { $0 == id }
    }

    private func emit(_ event: Event) {
        for id in observerOrder {
            observers[id]?(event)
        }
    }

    // MARK: - Sessions

    /// Starts `text` (under `skill`) for `origin`. Refused while the owner's rules cannot be
    /// used, while a session runs, and while a workflow runs unless the session is one of
    /// its steps. The returned task ends after
    /// `.sessionFinished` went to every observer.
    public func start(_ text: String, skill: SkillContext? = nil, origin: TaskOrigin = .owner)
        -> Result<Task<TaskRunOutcome, Never>, Refusal> {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return .failure(.emptyTask) }
        if let why = settingsProblem() { return .failure(.settingsUnavailable(why)) }
        guard session == nil else { return .failure(.busy) }
        if let workflow, origin.workflow?.runId != workflow.runId {
            return .failure(.workflowRunning(workflow.name))
        }
        let current = Session(id: UUID(), text: trimmed, skill: skill, origin: origin)
        session = current
        emit(.sessionStarted(current))

        // Events arrive on the driver's executor; a stream keeps their order on the way to
        // the main actor (separate Tasks per event would not).
        let (events, sink) = AsyncStream.makeStream(of: RunEvent.self)
        let consumer = Task { @MainActor [weak self] in
            for await event in events {
                self?.emit(.session(current, event))
            }
        }
        let coordinator = coordinator
        let broker = broker
        let attention = attention
        let settings = settings()
        let task = Task { @MainActor [weak self] () -> TaskRunOutcome in
            let outcome = await coordinator.run(task: trimmed, settings: settings, skill: skill, origin: origin,
                                                human: broker, attention: attention) { _ = sink.yield($0) }
            sink.finish()
            await consumer.value
            self?.sessionEnded(current, outcome)
            return outcome
        }
        sessionTask = task
        return .success(task)
    }

    /// Runs a session to the end (a workflow step). Cancelling the calling task stops the
    /// session.
    public func run(_ text: String, skill: SkillContext?, origin: TaskOrigin) async -> Result<TaskRunOutcome, Refusal> {
        switch start(text, skill: skill, origin: origin) {
        case .failure(let refusal):
            return .failure(refusal)
        case .success(let task):
            let outcome = await withTaskCancellationHandler {
                await task.value
            } onCancel: {
                task.cancel()
            }
            return .success(outcome)
        }
    }

    private func sessionEnded(_ ended: Session, _ outcome: TaskRunOutcome) {
        guard session?.id == ended.id else { return }
        session = nil
        sessionTask = nil
        broker.cancelAll()
        emit(.sessionFinished(ended, outcome))
    }

    /// Stops the running session (the driver delivers Cancel to the core at once).
    public func cancelSession() {
        sessionTask?.cancel()
        broker.cancelAll()
    }

    // MARK: - Workflows

    /// Takes the workflow slot (refused while anything runs, and while the owner's rules
    /// cannot be used).
    public func beginWorkflow(_ slot: WorkflowSlot) -> Refusal? {
        if let why = settingsProblem() { return .settingsUnavailable(why) }
        if let workflow { return .workflowRunning(workflow.name) }
        if session != nil { return .busy }
        workflow = slot
        emit(.workflowStarted(slot))
        return nil
    }

    /// Gives the slot back and reports the outcome.
    public func endWorkflow(_ slot: WorkflowSlot, outcome: WorkflowOutcome) {
        guard workflow?.runId == slot.runId else { return }
        workflow = nil
        emit(.workflowFinished(slot, outcome))
    }
}

extension TaskRunOutcome {
    /// The core's outcome; a run that could not start reads as Failed with the reason.
    public var outcome: Outcome {
        switch self {
        case .finished(_, let result): result.outcome
        case .notStarted(let why): Outcome(status: .failed, summary: why, steps: 0, durationMs: 0)
        }
    }
}
