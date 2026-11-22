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
///
/// The state file follows the store rules (`StoredFileRead`): while it cannot be read
/// (before the first unlock, an I/O error) nothing fires, because what already fired is
/// unknown, and nothing is written; every check reads it again. A damaged file is set aside
/// (`loadIssue` says where), and minutes up to the moment it was found are treated as
/// fired: a missed catch-up is better than a workflow running twice.
public final class WorkflowScheduler: Sendable {
    public let library: WorkflowLibrary
    public let catchUpMinutes: UInt32
    private let stateURL: URL?
    private let now: @Sendable () -> Date

    private struct State: Codable {
        /// Workflow id → the last scheduled minute that fired.
        var fired: [String: Int64] = [:]
        /// Minutes up to this one count as fired for every workflow (set when a damaged
        /// state file was set aside).
        var firedThrough: Int64?
    }

    private struct Guarded {
        /// `nil` while the state file cannot be read.
        var state: State?
        /// Marked while the file could not be read; merged in once it can.
        var pending: [String: Int64] = [:]
        var issue: String?
    }

    private let guarded: Mutex<Guarded>

    public init(library: WorkflowLibrary, stateURL: URL? = nil, catchUpMinutes: UInt32 = 15,
                now: @escaping @Sendable () -> Date = Date.init) {
        self.library = library
        self.stateURL = stateURL
        self.catchUpMinutes = max(1, catchUpMinutes)
        self.now = now
        var g = Guarded()
        if stateURL == nil { g.state = State() }
        self.guarded = Mutex(g)
        if stateURL != nil { guarded.withLock { loadIfNeeded(&$0) } }
    }

    /// Why the schedule state could not be used as it was (unreadable now, or set aside).
    public var loadIssue: String? { guarded.withLock { $0.issue } }

    /// Reads the state file when it has not been read yet (called under the lock).
    private func loadIfNeeded(_ g: inout Guarded) {
        guard g.state == nil, let stateURL else { return }
        var loaded: State
        var write = false
        switch StoredFile.read(stateURL, now: now, decode: { try JSONDecoder().decode(State.self, from: $0) }) {
        case .loaded(let state):
            loaded = state
            g.issue = nil
        case .missing:
            loaded = State()
            g.issue = nil
        case .unreadable(let reason):
            g.issue = "The schedule record could not be read (\(reason)); scheduled workflows wait until it can be."
            return
        case .damaged(let reason, let setAside):
            guard let setAside else {
                g.issue = "The schedule record is damaged (\(reason)) and could not be set aside; scheduled workflows wait."
                return
            }
            loaded = State(fired: [:], firedThrough: Int64((now().timeIntervalSince1970 / 60).rounded(.down)))
            write = true
            g.issue = "The schedule record was damaged (\(reason)) and was kept as \(setAside.lastPathComponent); "
                + "schedules missed before now are not caught up."
        }
        for (id, minute) in g.pending { loaded.fired[id] = max(loaded.fired[id] ?? .min, minute) }
        if !g.pending.isEmpty { write = true }
        g.pending = [:]
        g.state = loaded
        if write { persist(loaded) }
    }

    /// The scheduled minutes due at `time` that have not fired, ordered by workflow id.
    /// Empty while the state file cannot be read.
    public func due(at time: LocalTime) -> [ScheduledFire] {
        guard let state = guarded.withLock({ g -> State? in loadIfNeeded(&g); return g.state }) else { return [] }
        let candidates = library.due(at: time, windowMinutes: catchUpMinutes)
        return candidates.compactMap { workflow in
            guard case .schedule(let schedule) = workflow.trigger,
                  let since = try? scheduleMinutesSinceLast(schedule: schedule, weekday: time.weekday, minuteOfDay: time.minuteOfDay)
            else { return nil }
            let minute = time.epochMinute - Int64(since)
            if let last = state.fired[workflow.id], last >= minute { return nil }
            if let through = state.firedThrough, through >= minute { return nil }
            // Saved or switched on after that minute ended: not a missed run.
            if Int64(clamping: workflow.updatedMs) >= (minute + 1) * 60_000 { return nil }
            return ScheduledFire(workflow: workflow, scheduledMinute: minute)
        }
    }

    /// Remembers that `fire` ran (or must not be retried). While the state file cannot be
    /// read it is kept in memory and merged into the file once it can be read.
    public func markFired(_ fire: ScheduledFire) {
        let snapshot = guarded.withLock { g -> State? in
            loadIfNeeded(&g)
            guard var s = g.state else {
                g.pending[fire.workflow.id] = max(g.pending[fire.workflow.id] ?? .min, fire.scheduledMinute)
                return nil
            }
            s.fired[fire.workflow.id] = max(s.fired[fire.workflow.id] ?? .min, fire.scheduledMinute)
            let horizon = fire.scheduledMinute - 8 * 24 * 60
            s.fired = s.fired.filter { $0.value >= horizon }
            if let through = s.firedThrough, through < horizon { s.firedThrough = nil }
            g.state = s
            return s
        }
        if let snapshot { persist(snapshot) }
    }

    private func persist(_ state: State) {
        guard let stateURL, let data = try? JSONEncoder().encode(state) else { return }
        try? StoredFile.write(data, to: stateURL)
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
            case .failure(.alreadyRunning), .failure(.busy), .failure(.settingsUnavailable):
                // Tried again at the next check, inside the window.
                return nil
            case .failure(.notFound), .failure(.core):
                markFired(fire)
            }
        }
        return nil
    }
}
