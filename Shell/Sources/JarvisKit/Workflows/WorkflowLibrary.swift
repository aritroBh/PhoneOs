import Foundation
import JarvisCore
import Synchronization

/// Why a `WorkflowLibrary` call failed.
public enum WorkflowLibraryError: Error, Equatable, Sendable, CustomStringConvertible {
    /// The core refused the workflow or the lookup (`JarvisError.InvalidWorkflow`); the
    /// message is the core's, verbatim. Show it next to `field`.
    case invalidWorkflow(String)
    /// Another core error, verbatim.
    case core(String)
    /// Reading or writing the file failed, or writing is blocked because an unreadable file
    /// could not be set aside.
    case io(String)

    public init(core error: any Error) {
        let message = CoreErrorMessage(error)
        self = message.isInvalidWorkflow ? .invalidWorkflow(message.message) : .core(message.message)
    }

    public var description: String {
        switch self {
        case .invalidWorkflow(let m), .core(let m): m
        case .io(let m): "workflows file: \(m)"
        }
    }

    public var field: WorkflowField {
        guard case .invalidWorkflow(let message) = self else { return .general }
        return WorkflowField(coreMessage: message)
    }
}

/// What Ask does with a request, decided by the core's `resolveUtterance` (one rule for
/// workflow phrases and skill triggers).
public enum AskRoute: Sendable, Equatable {
    /// Nothing matched: run the words as a plain task.
    case plain
    /// A workflow's phrase matched exactly and nothing competes: run it.
    case runWorkflow(Workflow)
    /// One exact, unambiguous skill match with everything filled.
    case runSkill(SkillCandidate, SkillContext)
    /// One exact skill match that lacks required values: ask for them, then run.
    case askArguments(SkillCandidate)
    /// The core wants the owner to choose: offer these plus "just do what I said".
    case choose(workflows: [Workflow], skills: [SkillCandidate])
}

/// The owner's workflows in one JSON file, backed by the core's `JarvisWorkflows`. Same
/// rules as `SkillLibrary`: atomic writes with file protection, damaged entries kept and
/// written back, a file refused as a whole set aside (never deleted) or, if that fails,
/// saving blocked. Skill checks (warnings, runs, Ask) use the live `JarvisSkills` of
/// `skills`. Lock order: this library's lock, then the skill library's.
public final class WorkflowLibrary: Sendable {
    public let fileURL: URL
    public let skills: SkillLibrary
    private let now: @Sendable () -> Date

    private struct State {
        var store: JarvisWorkflows
        var shellIssues: [String]
        var writeBlocked: String?
    }

    private let state: Mutex<State>

    /// Loads `fileURL` (missing means empty). Never throws: problems become `loadIssues`.
    public init(fileURL: URL, skills: SkillLibrary, now: @escaping @Sendable () -> Date = Date.init) {
        self.fileURL = fileURL
        self.skills = skills
        self.now = now
        self.state = Mutex(Self.load(fileURL: fileURL, now: now))
    }

    private static func load(fileURL: URL, now: () -> Date) -> State {
        let empty = { () -> JarvisWorkflows in
            // The core accepts the empty snapshot by contract ("empty for a new store").
            // swiftlint:disable:next force_try
            try! JarvisWorkflows(json: "")
        }
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return State(store: empty(), shellIssues: [], writeBlocked: nil)
        }
        let text: String
        do {
            let data = try Data(contentsOf: fileURL)
            guard let decoded = String(data: data, encoding: .utf8) else {
                return setAside(fileURL, reason: "it is not UTF-8 text", now: now, empty: empty())
            }
            text = decoded
        } catch {
            let why = "cannot read \(fileURL.lastPathComponent): \(error)"
            return State(store: empty(), shellIssues: ["The workflows file could not be read (\(why)). Saving is off so it is not overwritten."],
                         writeBlocked: why)
        }
        do {
            return State(store: try JarvisWorkflows(json: text), shellIssues: [], writeBlocked: nil)
        } catch {
            return setAside(fileURL, reason: CoreErrorMessage(error).message, now: now, empty: empty())
        }
    }

    private static func setAside(_ fileURL: URL, reason: String, now: () -> Date, empty: JarvisWorkflows) -> State {
        let fm = FileManager.default
        let stamp = (now().timeIntervalSince1970 * 1_000).clampedInt
        var target = fileURL.appendingPathExtension("corrupt-\(stamp)")
        var n = 1
        while fm.fileExists(atPath: target.path) {
            target = fileURL.appendingPathExtension("corrupt-\(stamp)-\(n)")
            n += 1
        }
        do {
            try fm.moveItem(at: fileURL, to: target)
            return State(store: empty, shellIssues: [
                "The workflows file could not be loaded (\(reason)). It was kept as \(target.lastPathComponent); nothing was deleted.",
            ], writeBlocked: nil)
        } catch {
            return State(store: empty, shellIssues: [
                "The workflows file could not be loaded (\(reason)) and could not be set aside (\(error)). Saving is off so it is not overwritten.",
            ], writeBlocked: "the unreadable workflows file could not be set aside")
        }
    }

    /// Reads the file again when saving is blocked because it could not be read at launch
    /// (before the first unlock after a restart, an I/O error) or could not be set aside.
    /// The owner's workflows then appear and saving works, without a relaunch. A library whose
    /// file was loaded is not re-read (nothing in memory is lost); while blocked, nothing
    /// could change in memory. Returns whether the workflows were re-read and saving now works.
    @discardableResult
    public func reloadIfBlocked() -> Bool {
        state.withLock { s in
            guard s.writeBlocked != nil else { return false }
            let fresh = Self.load(fileURL: fileURL, now: now)
            s = fresh
            return fresh.writeBlocked == nil
        }
    }

    /// Whether saving is blocked (the file could not be read or set aside).
    public var isWriteBlocked: Bool { state.withLock { $0.writeBlocked != nil } }

    // MARK: - Reading

    /// All (valid) workflows, ordered by id.
    public var workflows: [Workflow] {
        state.withLock { (try? $0.store.list()) ?? [] }
    }

    public func workflow(id: String) -> Workflow? {
        state.withLock { (try? $0.store.get(id: id)) ?? nil }
    }

    /// What loading reported, one line each (a file set aside, unknown fields, entries the
    /// core could not load and keeps unchanged).
    public var loadIssues: [String] {
        state.withLock { $0.shellIssues + ((try? $0.store.loadIssues()) ?? []) }
    }

    public var hasLoadIssues: Bool { !loadIssues.isEmpty }

    /// Problems that will fail a step when `workflow` runs (missing or disabled skills,
    /// undeclared or missing values), checked against the skills as they are now.
    public func warnings(for workflow: Workflow) -> [String] {
        let skills = self.skills
        return state.withLock { s in
            (try? skills.withStore { try s.store.warnings(workflow: workflow, skills: $0) }) ?? []
        }
    }

    /// Warnings for a form that is not saved yet.
    public func warnings(for draft: WorkflowDraft) -> [String] {
        warnings(for: Workflow(id: draft.id ?? "draft", name: draft.name, enabled: draft.enabled, trigger: draft.trigger.core,
                               steps: draft.coreSteps, onFailure: draft.onFailure, createdMs: 0, updatedMs: 0,
                               runs: 0, lastRunMs: nil, lastOutcome: nil))
    }

    /// Enabled workflows whose external trigger has `key`.
    public func forExternal(key: String) -> [Workflow] {
        state.withLock { (try? $0.store.forExternal(key: key)) ?? [] }
    }

    /// Enabled scheduled workflows that fired within the last `windowMinutes` at `time`.
    public func due(at time: LocalTime, windowMinutes: UInt32) -> [Workflow] {
        state.withLock { (try? $0.store.due(weekday: time.weekday, minuteOfDay: time.minuteOfDay, windowMinutes: windowMinutes)) ?? [] }
    }

    /// Minutes from `time` until `workflow` next runs on its schedule (`nil` when it is not
    /// scheduled or is off).
    public static func minutesUntilNextRun(of workflow: Workflow, at time: LocalTime) -> UInt32? {
        guard workflow.enabled, case .schedule(let schedule) = workflow.trigger else { return nil }
        return try? scheduleMinutesUntilNext(schedule: schedule, weekday: time.weekday, minuteOfDay: time.minuteOfDay)
    }

    /// Enabled workflows, for remote control's catalog.
    public var catalog: [CatalogEntry] {
        workflows.filter(\.enabled).map { w in
            CatalogEntry(id: w.id, name: w.name, summary: "\(w.steps.count) \(w.steps.count == 1 ? "step" : "steps")")
        }
    }

    /// The workflow a Shortcuts automation names: its external key first, then its id, then
    /// its name (ignoring case). Disabled workflows are not found by key (the core skips them).
    public func find(_ key: String) -> Workflow? {
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        if let byKey = forExternal(key: trimmed).first { return byKey }
        if let byId = workflow(id: trimmed) { return byId }
        return workflows.first { $0.name.compare(trimmed, options: [.caseInsensitive]) == .orderedSame }
    }

    public static let unreadableAnswer = "Jarvis cannot read its workflows until the phone has been unlocked once."

    /// Why a Shortcuts automation (or Siri) cannot run a workflow now, before looking it up:
    /// the owner's rules cannot be used (`settingsRefusal`, the gate's words), or the
    /// workflows file could not be read (`workflowsBlocked`, typically before the first unlock
    /// after a restart; "there is no workflow" would be wrong then). `nil` when it may run.
    public static func externalUnavailable(settingsRefusal: String?, workflowsBlocked: Bool) -> String? {
        if let settingsRefusal { return settingsRefusal }
        return workflowsBlocked ? unreadableAnswer : nil
    }

    // MARK: - Ask

    /// What to do with the owner's words: `resolveUtterance` decides between a workflow, a
    /// skill, the owner's choice and a plain task; this maps its answer to the next screen.
    public func resolve(_ text: String) throws -> AskRoute {
        let skills = self.skills
        return try state.withLock { s in
            do {
                return try skills.withStore { store -> AskRoute in
                    let match = try resolveUtterance(skills: store, workflows: s.store, text: text)
                    let candidates = try match.skills.compactMap { m in
                        try store.get(id: m.skillId).map { SkillCandidate(match: m, skill: $0) }
                    }
                    switch match.resolution {
                    case .noMatch:
                        return .plain
                    case .runWorkflow(let id):
                        guard let workflow = try s.store.get(id: id) else { return .plain }
                        return .runWorkflow(workflow)
                    case .runSkill(let m):
                        guard let skill = try store.get(id: m.skillId) else { return .plain }
                        let candidate = SkillCandidate(match: m, skill: skill)
                        if !candidate.missingParams.isEmpty { return .askArguments(candidate) }
                        return .runSkill(candidate, try store.contextForMatch(m: m))
                    case .ownerChoice:
                        let workflows = try match.workflows.compactMap { try s.store.get(id: $0) }
                        if workflows.isEmpty && candidates.isEmpty { return .plain }
                        return .choose(workflows: workflows, skills: candidates)
                    }
                }
            } catch let error as WorkflowLibraryError {
                throw error
            } catch {
                throw WorkflowLibraryError(core: error)
            }
        }
    }

    // MARK: - Changing

    /// Creates (draft without id: an id from the name) or updates a workflow and saves the
    /// file. Invalid drafts throw `.invalidWorkflow` with the core's reason; nothing is
    /// saved then.
    @discardableResult
    public func save(_ draft: WorkflowDraft) throws -> Workflow {
        let nowMs = self.nowMs
        return try mutate { store in
            let id: String
            if let given = draft.id {
                id = given
            } else {
                let taken = Set(try store.list().map(\.id) + (try store.damagedIds().compactMap { $0 }))
                id = SkillLibrary.makeID(from: draft.name, taken: taken, fallback: "workflow")
            }
            let workflow = Workflow(id: id, name: draft.name, enabled: draft.enabled, trigger: draft.trigger.core,
                                    steps: draft.coreSteps, onFailure: draft.onFailure, createdMs: nowMs, updatedMs: nowMs,
                                    runs: 0, lastRunMs: nil, lastOutcome: nil)
            _ = try store.upsert(workflow: workflow, nowMs: nowMs)
            return try store.get(id: id) ?? workflow
        }
    }

    public func setEnabled(_ enabled: Bool, id: String) throws {
        let nowMs = self.nowMs
        try mutate { store in try store.setEnabled(id: id, enabled: enabled, nowMs: nowMs) }
    }

    /// Removes a workflow (or a damaged entry kept with that id). False when there was neither.
    @discardableResult
    public func remove(id: String) throws -> Bool {
        try mutate { store in try store.remove(id: id) }
    }

    /// Records a finished run.
    public func recordRun(id: String, outcome: WorkflowOutcome) throws {
        let nowMs = self.nowMs
        try mutate { store in try store.recordRun(id: id, nowMs: nowMs, outcome: outcome) }
    }

    // MARK: - Runs

    /// A run of `id` as stored now (refused by the core at `start` when disabled).
    func makeRun(id: String, limits: RunLimits) throws -> JarvisWorkflowRun {
        let workflow: Workflow = try state.withLock { s in
            do {
                guard let w = try s.store.get(id: id) else {
                    throw WorkflowLibraryError.invalidWorkflow("no workflow with id \"\(id)\"")
                }
                return w
            } catch let error as WorkflowLibraryError {
                throw error
            } catch {
                throw WorkflowLibraryError(core: error)
            }
        }
        do {
            return try JarvisWorkflowRun(workflow: workflow, limits: limits)
        } catch {
            throw WorkflowLibraryError(core: error)
        }
    }

    // MARK: - Internals

    private var nowMs: UInt64 {
        UInt64(max(0, (now().timeIntervalSince1970 * 1_000).rounded()))
    }

    private func mutate<T: Sendable>(_ change: (JarvisWorkflows) throws -> T) throws -> T {
        try state.withLock { s in
            if let blocked = s.writeBlocked { throw WorkflowLibraryError.io(blocked) }
            let value: T
            let json: String
            do {
                value = try change(s.store)
                json = try s.store.toJson()
            } catch let error as WorkflowLibraryError {
                throw error
            } catch {
                throw WorkflowLibraryError(core: error)
            }
            do {
                try FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
                try Data(json.utf8).write(to: fileURL, options: Self.writeOptions)
            } catch {
                throw WorkflowLibraryError.io("cannot write \(fileURL.lastPathComponent): \(error)")
            }
            return value
        }
    }

    private static var writeOptions: Data.WritingOptions {
        #if canImport(Darwin)
        return [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
        #else
        return [.atomic]
        #endif
    }
}

/// Local wall time as the core's schedules want it.
public struct LocalTime: Sendable, Equatable {
    public var weekday: Weekday
    /// 0 to 1439.
    public var minuteOfDay: UInt32
    /// Whole minutes since 1970 (UTC), to tell scheduled minutes apart.
    public var epochMinute: Int64
    /// The local date as a day number (days since 1970-01-01 in the local calendar), so a
    /// local slot (date and minute) can be told apart from the UTC minute: the same local
    /// 01:30 happens twice on the night the clocks go back, and again after flying west.
    public var localDay: Int64
    /// The time zone the wall time was read in (`nil`: not known, no daylight-saving
    /// adjustment). The scheduler uses it to widen its catch-up window over a skipped hour.
    public var timeZone: TimeZone?

    /// `localDay` defaults to the day of `epochMinute - minuteOfDay` (UTC arithmetic).
    public init(weekday: Weekday, minuteOfDay: UInt32, epochMinute: Int64, localDay: Int64? = nil, timeZone: TimeZone? = nil) {
        self.weekday = weekday
        self.minuteOfDay = minuteOfDay
        self.epochMinute = epochMinute
        self.localDay = localDay ?? Self.floorDiv(epochMinute - Int64(minuteOfDay), 1_440)
        self.timeZone = timeZone
    }

    /// `date` in `calendar`'s time zone.
    public init(date: Date, calendar: Calendar = .current) {
        let parts = calendar.dateComponents([.year, .month, .day, .weekday, .hour, .minute], from: date)
        // Calendar: 1 = Sunday … 7 = Saturday; the core: 0 = Monday.
        let index = UInt32(((parts.weekday ?? 2) + 5) % 7)
        self.weekday = (try? weekdayFromIndex(index: index)) ?? .mon
        self.minuteOfDay = UInt32(max(0, min(1_439, (parts.hour ?? 0) * 60 + (parts.minute ?? 0))))
        self.epochMinute = Int64((date.timeIntervalSince1970 / 60).rounded(.down))
        if let y = parts.year, let m = parts.month, let d = parts.day {
            self.localDay = Self.daysFromCivil(year: Int64(y), month: Int64(m), day: Int64(d))
        } else {
            self.localDay = Self.floorDiv(epochMinute - Int64(minuteOfDay), 1_440)
        }
        self.timeZone = calendar.timeZone
    }

    /// The local slot of this time: local day × 1440 + minute of day.
    public var localMinute: Int64 { localDay * 1_440 + Int64(minuteOfDay) }

    static func floorDiv(_ a: Int64, _ b: Int64) -> Int64 {
        let q = a / b
        return (a % b != 0 && (a < 0) != (b < 0)) ? q - 1 : q
    }

    /// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant's algorithm).
    static func daysFromCivil(year: Int64, month: Int64, day: Int64) -> Int64 {
        let y = month <= 2 ? year - 1 : year
        let era = floorDiv(y, 400)
        let yoe = y - era * 400
        let mp = (month + 9) % 12
        let doy = (153 * mp + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }
}

/// Workflows for the simulator tests (launch arguments save them through the library, so
/// the core writes the file).
public enum WorkflowSeeds {
    /// Every day at 05:30, one notify step: the list shows "Next run".
    public static let scheduled = WorkflowDraft(
        name: "Morning note",
        trigger: TriggerDraft(kind: .schedule, days: Set(WeekdayOption.allCases), hour: 5, minute: 30),
        steps: [StepDraft(kind: .notify, text: "Good morning")]
    )
}
