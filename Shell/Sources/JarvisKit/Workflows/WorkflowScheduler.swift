import Foundation
import JarvisCore
import Synchronization

/// One scheduled minute of one workflow that is due.
public struct ScheduledFire: Sendable, Equatable {
    public var workflow: Workflow
    /// The scheduled minute (minutes since 1970, UTC).
    public var scheduledMinute: Int64
}

/// Fires scheduled workflows while Jarvis runs. iOS does not wake an app at a set time, so
/// this only works while Jarvis is open (or kept alive); exact times need a Shortcuts
/// automation calling `RunJarvisWorkflowIntent`.
///
/// Each check asks the core which schedules fired within the last `catchUpMinutes`
/// (`JarvisWorkflows.due`), and fires each scheduled minute at most once: fired minutes are
/// remembered (in `stateURL` when given), so a relaunch inside the window catches up once
/// and never twice. A minute that ended before the workflow was last saved or switched on
/// is not caught up. While something else runs, a due workflow waits for the next check
/// (inside the window).
public final class WorkflowScheduler: Sendable {
    public let library: WorkflowLibrary
    public let catchUpMinutes: UInt32
    private let stateURL: URL?

    private struct State: Codable {
        /// Workflow id → the last scheduled minute that fired.
        var fired: [String: Int64] = [:]
    }

    private let state: Mutex<State>

    public init(library: WorkflowLibrary, stateURL: URL? = nil, catchUpMinutes: UInt32 = 15) {
        self.library = library
        self.stateURL = stateURL
        self.catchUpMinutes = max(1, catchUpMinutes)
        var loaded = State()
        if let stateURL, let data = try? Data(contentsOf: stateURL),
           let decoded = try? JSONDecoder().decode(State.self, from: data) {
            loaded = decoded
        }
        self.state = Mutex(loaded)
    }

    /// The scheduled minutes due at `time` that have not fired, ordered by workflow id.
    public func due(at time: LocalTime) -> [ScheduledFire] {
        let candidates = library.due(at: time, windowMinutes: catchUpMinutes)
        let fired = state.withLock { $0.fired }
        return candidates.compactMap { workflow in
            guard case .schedule(let schedule) = workflow.trigger,
                  let since = try? scheduleMinutesSinceLast(schedule: schedule, weekday: time.weekday, minuteOfDay: time.minuteOfDay)
            else { return nil }
            let minute = time.epochMinute - Int64(since)
            if let last = fired[workflow.id], last >= minute { return nil }
            // Saved or switched on after that minute ended: not a missed run.
            if Int64(clamping: workflow.updatedMs) >= (minute + 1) * 60_000 { return nil }
            return ScheduledFire(workflow: workflow, scheduledMinute: minute)
        }
    }

    /// Remembers that `fire` ran (or must not be retried).
    public func markFired(_ fire: ScheduledFire) {
        let snapshot = state.withLock { s -> State in
            s.fired[fire.workflow.id] = max(s.fired[fire.workflow.id] ?? .min, fire.scheduledMinute)
            let horizon = fire.scheduledMinute - 8 * 24 * 60
            s.fired = s.fired.filter { $0.value >= horizon }
            return s
        }
        guard let stateURL, let data = try? JSONEncoder().encode(snapshot) else { return }
        try? FileManager.default.createDirectory(at: stateURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: stateURL, options: [.atomic])
    }

    /// Starts the first due workflow that can start now. A workflow refused because
    /// something else runs is tried again at the next check; one that cannot run at all
    /// (removed, invalid) is marked so it is not retried. Returns the id started, if any.
    @MainActor
    @discardableResult
    public func tick(at time: LocalTime, runner: WorkflowRunner) -> String? {
        for fire in due(at: time) {
            switch runner.start(id: fire.workflow.id, origin: .owner) {
            case .success:
                markFired(fire)
                return fire.workflow.id
            case .failure(.alreadyRunning), .failure(.busy):
                return nil
            case .failure(.notFound), .failure(.core):
                markFired(fire)
            }
        }
        return nil
    }
}
