import Foundation
import JarvisCore
import Synchronization

/// One scheduled minute of one workflow that is due.
public struct ScheduledFire: Sendable, Equatable {
    public var workflow: Workflow
    /// The scheduled minute (minutes since 1970, UTC).
    public var scheduledMinute: Int64
    /// The scheduled local slot (`LocalTime.localMinute`: local day × 1440 + minute of day),
    /// `nil` when not known.
    public var localSlot: Int64?

    public init(workflow: Workflow, scheduledMinute: Int64, localSlot: Int64? = nil) {
        self.workflow = workflow
        self.scheduledMinute = scheduledMinute
        self.localSlot = localSlot
    }
}

/// Fires scheduled workflows while Jarvis runs. iOS does not wake an app at a set time, so
/// this only works while Jarvis is open (or kept alive); exact times need a Shortcuts
/// automation calling `RunJarvisWorkflowIntent`.
///
/// Each check asks the core which schedules fired within the last `catchUpMinutes`
/// (`JarvisWorkflows.due`), and fires each scheduled minute at most once: fired minutes are
/// remembered (in `stateURL` when given), so a relaunch inside the window catches up once
/// and never twice. Per workflow it keeps the latest fired UTC minute and the latest fired
/// local slot (local date and minute): a fire is refused when its UTC minute is not after
/// the one that fired, or when its local slot equals the one that fired (the night the
/// clocks go back, 01:30 happens twice; flying west repeats local times). These entries are
/// never pruned by time, so a clock that ran ahead and was set back cannot make a minute
/// fire again; only entries of workflows that no longer exist are dropped. A minute that
/// ended before the workflow was last saved or switched on is not caught up. While
/// something else runs, a due workflow waits for the next check (inside the window).
///
/// Daylight saving: the catch-up window is real time. When the clocks go forward inside
/// the window, the window on the wall clock is widened by the skipped minutes, so a
/// schedule in the skipped hour (02:30 on the night 02:00 becomes 03:00) fires once, at the
/// first check after the gap (03:00), and is not lost.
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
        /// Workflow id → the local slot of that fire (`LocalTime.localMinute`). Absent in
        /// files written before it existed.
        var firedSlot: [String: Int64] = [:]
        /// Minutes up to this one count as fired for every workflow (set when a damaged
        /// state file was set aside).
        var firedThrough: Int64?

        init(fired: [String: Int64] = [:], firedSlot: [String: Int64] = [:], firedThrough: Int64? = nil) {
            self.fired = fired
            self.firedSlot = firedSlot
            self.firedThrough = firedThrough
        }

        enum CodingKeys: String, CodingKey { case fired, firedSlot, firedThrough }

        init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            fired = try c.decode([String: Int64].self, forKey: .fired)
            firedSlot = try c.decodeIfPresent([String: Int64].self, forKey: .firedSlot) ?? [:]
            firedThrough = try c.decodeIfPresent(Int64.self, forKey: .firedThrough)
        }

        /// Records `fire` as the latest of its workflow (an older minute changes nothing).
        mutating func mark(_ id: String, minute: Int64, slot: Int64?) {
            if let last = fired[id], last > minute { return }
            fired[id] = minute
            if let slot { firedSlot[id] = slot }
        }
    }

    private struct Guarded {
        /// `nil` while the state file cannot be read.
        var state: State?
        /// Marked while the file could not be read; merged in once it can.
        var pending = State()
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
        for (id, minute) in g.pending.fired { loaded.mark(id, minute: minute, slot: g.pending.firedSlot[id]) }
        if !g.pending.fired.isEmpty { write = true }
        g.pending = State()
        g.state = loaded
        if write { persist(loaded) }
    }

    /// The scheduled minutes due at `time` that have not fired, ordered by workflow id.
    /// Empty while the state file cannot be read.
    public func due(at time: LocalTime) -> [ScheduledFire] {
        guard let state = guarded.withLock({ g -> State? in loadIfNeeded(&g); return g.state }) else { return [] }
        let candidates = library.due(at: time, windowMinutes: wallWindow(at: time))
        return candidates.compactMap { workflow in
            guard case .schedule(let schedule) = workflow.trigger,
                  let since = try? scheduleMinutesSinceLast(schedule: schedule, weekday: time.weekday, minuteOfDay: time.minuteOfDay)
            else { return nil }
            let minute = time.epochMinute - Int64(since)
            let slot = time.localMinute - Int64(since)
            if let last = state.fired[workflow.id], last >= minute { return nil }
            // The same local date and minute fired already (clocks went back, flew west).
            if let lastSlot = state.firedSlot[workflow.id], lastSlot == slot { return nil }
            if let through = state.firedThrough, through >= minute { return nil }
            // Saved or switched on after that minute ended: not a missed run.
            if Int64(clamping: workflow.updatedMs) >= (minute + 1) * 60_000 { return nil }
            return ScheduledFire(workflow: workflow, scheduledMinute: minute, localSlot: slot)
        }
    }

    /// The catch-up window on the wall clock: `catchUpMinutes`, plus the minutes the clocks
    /// skipped inside it (spring forward), so the window stays `catchUpMinutes` of real time.
    private func wallWindow(at time: LocalTime) -> UInt32 {
        guard let zone = time.timeZone else { return catchUpMinutes }
        let nowDate = Date(timeIntervalSince1970: Double(time.epochMinute) * 60)
        let start = nowDate.addingTimeInterval(-Double(catchUpMinutes) * 60)
        let skipped = (zone.secondsFromGMT(for: nowDate) - zone.secondsFromGMT(for: start)) / 60
        return skipped > 0 ? catchUpMinutes + UInt32(skipped) : catchUpMinutes
    }

    /// Remembers that `fire` ran (or must not be retried). While the state file cannot be
    /// read it is kept in memory and merged into the file once it can be read.
    /// Entries of workflows that no longer exist are dropped (never by age).
    public func markFired(_ fire: ScheduledFire) {
        // Read before taking this scheduler's lock (the library has its own).
        let existing = Set(library.workflows.map(\.id))
        let snapshot = guarded.withLock { g -> State? in
            loadIfNeeded(&g)
            guard var s = g.state else {
                g.pending.mark(fire.workflow.id, minute: fire.scheduledMinute, slot: fire.localSlot)
                return nil
            }
            s.mark(fire.workflow.id, minute: fire.scheduledMinute, slot: fire.localSlot)
            let keep = existing.union([fire.workflow.id])
            s.fired = s.fired.filter { keep.contains($0.key) }
            s.firedSlot = s.firedSlot.filter { keep.contains($0.key) }
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
