import Foundation
import JarvisCore
import Synchronization

/// Why a `SkillLibrary` call failed.
public enum SkillLibraryError: Error, Equatable, Sendable, CustomStringConvertible {
    /// The core refused the skill, the lookup or the promotion (`JarvisError.InvalidSkill`);
    /// the message is the core's, verbatim. Show it next to `field`.
    case invalidSkill(String)
    /// Another core error (for example memory bytes the core cannot read), verbatim.
    case core(String)
    /// Reading or writing the file failed, or writing is blocked because an unreadable file
    /// could not be set aside.
    case io(String)

    public init(core error: any Error) {
        let message = CoreErrorMessage(error)
        self = message.isInvalidSkill ? .invalidSkill(message.message) : .core(message.message)
    }

    public var description: String {
        switch self {
        case .invalidSkill(let m), .core(let m): m
        case .io(let m): "skills file: \(m)"
        }
    }

    /// The form field the core's message is about (the core names the field first, for
    /// example `name: must not be empty` or `trigger "…" uses {who}, …`).
    public var field: SkillField {
        guard case .invalidSkill(let message) = self else { return .general }
        return SkillField(coreMessage: message)
    }
}

/// The parts of the skill form a core message can be about.
public enum SkillField: String, Sendable, Equatable, CaseIterable {
    case name, summary, triggers, instructions, apps, tools, general

    /// Reads which field a core validation message names. This only routes the message to
    /// a place on screen; the validation itself is the core's.
    public init(coreMessage message: String) {
        let m = message.lowercased()
        if m.hasPrefix("name") || m.hasPrefix("invalid id") || m.hasPrefix("id ") {
            self = .name
        } else if m.hasPrefix("summary") {
            self = .summary
        } else if m.hasPrefix("trigger") || m.hasPrefix("param") || m.hasPrefix("parameter") || m.hasPrefix("{") {
            self = .triggers
        } else if m.hasPrefix("instructions") {
            self = .instructions
        } else if m.hasPrefix("apps") || m.hasPrefix("confine") {
            self = .apps
        } else if m.hasPrefix("tools") {
            self = .tools
        } else {
            self = .general
        }
    }
}

/// What the owner edits in the skill form. Becomes a core `Skill` in
/// `SkillLibrary.save(_:)`; every rule about it is checked by the core there.
public struct SkillDraft: Sendable, Equatable {
    /// `nil` for a new skill (an id is made from the name).
    public var id: String?
    public var name: String
    public var summary: String
    /// Phrases, `{slot}` for a value ("text {contact} {message}").
    public var triggers: [String]
    public var instructions: String
    /// Bundle ids the skill works in.
    public var apps: [String]
    public var confineToApps: Bool
    /// App-less tools a confined skill may still call.
    public var tools: [String]
    public var enabled: Bool

    public init(id: String? = nil, name: String = "", summary: String = "", triggers: [String] = [""],
                instructions: String = "", apps: [String] = [], confineToApps: Bool = false,
                tools: [String] = [], enabled: Bool = true) {
        self.id = id
        self.name = name
        self.summary = summary
        self.triggers = triggers
        self.instructions = instructions
        self.apps = apps
        self.confineToApps = confineToApps
        self.tools = tools
        self.enabled = enabled
    }

    /// The draft for editing `skill`.
    public init(skill: Skill) {
        self.init(id: skill.id, name: skill.name, summary: skill.summary,
                  triggers: skill.triggers.isEmpty ? [""] : skill.triggers,
                  instructions: skill.instructions, apps: skill.apps, confineToApps: skill.confineToApps,
                  tools: skill.tools, enabled: skill.enabled)
    }

    /// The `{slot}` names in a trigger, in order of first appearance (text between braces,
    /// as typed: the core decides whether it is a valid parameter name).
    public static func slots(in trigger: String) -> [String] {
        var names: [String] = []
        var rest = Substring(trigger)
        while let open = rest.firstIndex(of: "{") {
            let after = rest[rest.index(after: open)...]
            guard let close = after.firstIndex(of: "}") else { break }
            let name = String(after[..<close])
            if !names.contains(name) { names.append(name) }
            rest = after[after.index(after: close)...]
        }
        return names
    }

    /// The triggers the owner filled in (empty rows of the form are left out).
    var filledTriggers: [String] {
        triggers.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
    }
}

/// One skill the matcher offered for a request, with what is still needed to run it.
public struct SkillCandidate: Sendable, Equatable, Identifiable {
    public var match: SkillMatch
    public var skill: Skill
    /// Required parameters the request did not fill, in the core's order.
    public var missingParams: [SkillParam]

    public var id: String { skill.id }

    public init(match: SkillMatch, skill: Skill) {
        self.match = match
        self.skill = skill
        self.missingParams = match.missing.map { name in
            skill.params.first { $0.name == name } ?? SkillParam(name: name, description: "", required: true)
        }
    }
}

/// What the Ask screen does with a request, decided by the core's matcher.
public enum SkillRoute: Sendable, Equatable {
    /// No skill matched: run the words as a plain task.
    case plain
    /// One exact, unambiguous match with everything filled: run with this skill.
    case run(SkillCandidate, SkillContext)
    /// One exact, unambiguous match that lacks required values: ask for each, then run.
    case askArguments(SkillCandidate)
    /// The core wants the owner to choose (ambiguous, or the best match is loose). Offer
    /// these plus "just do what I said".
    case choose([SkillCandidate])
}

/// The owner's skills in one JSON file, backed by the core's `JarvisSkills`.
///
/// - Writes are atomic (temporary file, then rename) and, on Apple platforms, get
///   complete-until-first-user-authentication protection, as `FileMemoryStore`.
/// - The core keeps damaged entries verbatim and writes them back, so saving never drops
///   them; `loadIssues` lists them (and unknown fields) for the owner.
/// - A file the core refuses as a whole (not JSON, a newer schema) is renamed aside
///   (`<name>.corrupt-<ms>`), never deleted, and the library starts empty with an issue
///   saying where the file went. If it cannot be renamed, saving is blocked so the file is
///   not overwritten.
/// - Calls are synchronous and serialised; the file is small (≤ 500 skills).
public final class SkillLibrary: Sendable {
    public let fileURL: URL
    private let now: @Sendable () -> Date

    private struct State {
        var store: JarvisSkills
        /// Issues found by the shell (a file set aside), shown before the core's.
        var shellIssues: [String]
        var writeBlocked: String?
    }

    private let state: Mutex<State>

    /// Loads `fileURL` (missing means empty). Never throws: problems become `loadIssues`.
    public init(fileURL: URL, now: @escaping @Sendable () -> Date = Date.init) {
        self.fileURL = fileURL
        self.now = now
        self.state = Mutex(Self.load(fileURL: fileURL, now: now))
    }

    private static func load(fileURL: URL, now: () -> Date) -> State {
        let empty = { () -> JarvisSkills in
            // The core accepts the empty snapshot by contract ("empty for a new store").
            // swiftlint:disable:next force_try
            try! JarvisSkills(json: "")
        }
        let fm = FileManager.default
        guard fm.fileExists(atPath: fileURL.path) else {
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
            return State(store: empty(), shellIssues: ["The skills file could not be read (\(why)). Saving is off so it is not overwritten."],
                         writeBlocked: why)
        }
        do {
            return State(store: try JarvisSkills(json: text), shellIssues: [], writeBlocked: nil)
        } catch {
            return setAside(fileURL, reason: CoreErrorMessage(error).message, now: now, empty: empty())
        }
    }

    private static func setAside(_ fileURL: URL, reason: String, now: () -> Date, empty: JarvisSkills) -> State {
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
                "The skills file could not be loaded (\(reason)). It was kept as \(target.lastPathComponent); nothing was deleted.",
            ], writeBlocked: nil)
        } catch {
            return State(store: empty, shellIssues: [
                "The skills file could not be loaded (\(reason)) and could not be set aside (\(error)). Saving is off so it is not overwritten.",
            ], writeBlocked: "the unreadable skills file could not be set aside")
        }
    }

    /// Reads the file again when saving is blocked because it could not be read at launch
    /// (before the first unlock after a restart, an I/O error) or could not be set aside.
    /// The owner's skills then appear and saving works, without a relaunch. A library whose
    /// file was loaded is not re-read (nothing in memory is lost); while blocked, nothing
    /// could change in memory. Returns whether the skills were re-read and saving now works.
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

    /// All skills, ordered by id.
    public var skills: [Skill] {
        state.withLock { (try? $0.store.list()) ?? [] }
    }

    public func skill(id: String) -> Skill? {
        state.withLock { (try? $0.store.get(id: id)) ?? nil }
    }

    /// What loading reported, one line each: a file set aside, unknown fields, and entries
    /// the core could not load (kept unchanged and written back on every save).
    public var loadIssues: [String] {
        state.withLock { $0.shellIssues + ((try? $0.store.loadIssues()) ?? []) }
    }

    public var hasLoadIssues: Bool { !loadIssues.isEmpty }

    /// The snapshot as it would be saved.
    public func snapshotJSON() throws -> String {
        try state.withLock { s in
            do { return try s.store.toJson() } catch { throw SkillLibraryError(core: error) }
        }
    }

    // MARK: - Changing

    /// Creates (draft without id: a Written skill with an id made from the name) or updates
    /// a skill and saves the file. Returns the stored skill. Invalid drafts throw
    /// `.invalidSkill` with the core's reason; nothing is saved then.
    @discardableResult
    public func save(_ draft: SkillDraft) throws -> Skill {
        let nowMs = self.nowMs
        return try mutate { store in
            let existing = try draft.id.flatMap { try store.get(id: $0) }
            let id: String
            if let given = draft.id {
                id = given
            } else {
                let taken = Set(try store.list().map(\.id))
                id = Self.makeID(from: draft.name, taken: taken)
            }
            let triggers = draft.filledTriggers
            var slotNames: [String] = []
            for trigger in triggers {
                for slot in SkillDraft.slots(in: trigger) where !slotNames.contains(slot) { slotNames.append(slot) }
            }
            // Slots in the triggers become parameters; ones the owner declared before keep
            // their description and requiredness, and declared parameters without a slot stay.
            var params = slotNames.map { name in
                existing?.params.first { $0.name == name } ?? SkillParam(name: name, description: "", required: true)
            }
            for p in existing?.params ?? [] where !params.contains(where: { $0.name == p.name }) { params.append(p) }
            let skill = Skill(
                id: id, name: draft.name, summary: draft.summary, kind: existing?.kind ?? .written,
                triggers: triggers, instructions: draft.instructions,
                apps: draft.apps.map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty },
                confineToApps: draft.confineToApps, params: params, flowKeys: existing?.flowKeys ?? [],
                enabled: draft.enabled, createdMs: existing?.createdMs ?? nowMs, updatedMs: nowMs,
                runs: existing?.runs ?? 0, lastRunMs: existing?.lastRunMs, tools: draft.tools
            )
            _ = try store.upsert(skill: skill)
            return try store.get(id: id) ?? skill
        }
    }

    /// Turns a skill on or off.
    public func setEnabled(_ enabled: Bool, id: String) throws {
        let nowMs = self.nowMs
        try mutate { store in
            guard var skill = try store.get(id: id) else {
                throw SkillLibraryError.invalidSkill("no item with id \"\(id)\"")
            }
            skill.enabled = enabled
            skill.updatedMs = nowMs
            _ = try store.upsert(skill: skill)
        }
    }

    /// Removes a skill (or a damaged entry kept with that id). False when there was neither.
    @discardableResult
    public func remove(id: String) throws -> Bool {
        try mutate { store in try store.remove(id: id) }
    }

    /// Records a finished run of skill `id`.
    public func recordRun(id: String) throws {
        let nowMs = self.nowMs
        try mutate { store in try store.recordRun(id: id, nowMs: nowMs) }
    }

    /// Makes the learned flow `flowId` from `memoryJSON` a Learned skill named `name`.
    @discardableResult
    public func promote(flowId: String, memoryJSON: String, name: String) throws -> Skill {
        let nowMs = self.nowMs
        return try mutate { store in
            let id = try store.promoteFlow(memoryJson: memoryJSON, flowId: flowId, name: name, nowMs: nowMs)
            guard let skill = try store.get(id: id) else {
                throw SkillLibraryError.invalidSkill("no item with id \"\(id)\"")
            }
            return skill
        }
    }

    // MARK: - Matching

    /// What to do with the owner's request: the core's matcher decides; this only maps its
    /// answer (`exact`, `needsOwnerChoice`, `missing`) to the next screen.
    public func route(_ text: String) throws -> SkillRoute {
        try state.withLock { s in
            do {
                let result = try s.store.matchTask(text: text)
                let candidates = try result.matches.compactMap { m in
                    try s.store.get(id: m.skillId).map { SkillCandidate(match: m, skill: $0) }
                }
                guard let best = candidates.first else { return .plain }
                if result.needsOwnerChoice || !best.match.exact { return .choose(candidates) }
                if !best.missingParams.isEmpty { return .askArguments(best) }
                return .run(best, try s.store.contextForMatch(m: best.match))
            } catch let error as SkillLibraryError {
                throw error
            } catch {
                throw SkillLibraryError(core: error)
            }
        }
    }

    /// The session's skill context for `candidate`, with `values` for the parameters the
    /// request left empty. The core checks it against the skill as it is now.
    public func context(for candidate: SkillCandidate, values: [String: String] = [:]) throws -> SkillContext {
        try state.withLock { s in
            do {
                if values.isEmpty && candidate.missingParams.isEmpty {
                    return try s.store.contextForMatch(m: candidate.match)
                }
                return try s.store.context(skillId: candidate.skill.id, args: candidate.match.args.merging(values) { _, new in new })
            } catch {
                throw SkillLibraryError(core: error)
            }
        }
    }

    /// The skill context for running skill `id` directly ("Run now"), with `values`.
    public func context(skillId id: String, values: [String: String] = [:]) throws -> SkillContext {
        try state.withLock { s in
            do { return try s.store.context(skillId: id, args: values) } catch { throw SkillLibraryError(core: error) }
        }
    }

    /// The task text for running skill `id` without the owner's words ("Run now").
    public func taskText(skillId id: String, values: [String: String] = [:]) throws -> String {
        try state.withLock { s in
            do { return try s.store.taskText(skillId: id, args: values) } catch { throw SkillLibraryError(core: error) }
        }
    }

    /// The id of the learned flow in `memoryJSON` that a finished task `text` would replay
    /// (`flowIDs` to consider, e.g. `MemorySnapshot.flows`), `nil` when none. The core
    /// decides: each flow is promoted into a scratch store and the task text is matched
    /// against it, so its intent-key folding ("turn off wi-fi" → "disable wifi") applies.
    public static func learnedFlowID(forTask text: String, memoryJSON: String, flowIDs: [String]) -> String? {
        guard !memoryJSON.isEmpty, let scratch = try? JarvisSkills(json: "") else { return nil }
        var flowForSkill: [String: String] = [:]
        for flowID in flowIDs {
            if let skillID = try? scratch.promoteFlow(memoryJson: memoryJSON, flowId: flowID, name: "flow", nowMs: 0) {
                flowForSkill[skillID] = flowID
            }
        }
        guard let best = (try? scratch.matchTask(text: text))?.matches.first, best.exact else { return nil }
        return flowForSkill[best.skillId]
    }

    // MARK: - Internals

    /// Runs `body` with the live core store (workflow warnings and runs read skills from
    /// it, so a skill changed mid-run counts at the next step). Lock order: a caller that
    /// holds `WorkflowLibrary`'s lock may take this one, never the reverse.
    func withStore<T: Sendable>(_ body: (JarvisSkills) throws -> T) throws -> T {
        try state.withLock { s in try body(s.store) }
    }

    /// Enabled skills, for remote control's catalog.
    public var catalog: [CatalogEntry] {
        skills.filter(\.enabled).map { CatalogEntry(id: $0.id, name: $0.name, summary: $0.summary) }
    }

    private var nowMs: UInt64 {
        UInt64(max(0, (now().timeIntervalSince1970 * 1_000).rounded()))
    }

    /// Runs `change` on the store, then writes the snapshot. Core errors become
    /// `SkillLibraryError`s with the core's words.
    private func mutate<T>(_ change: (JarvisSkills) throws -> T) throws -> T {
        try state.withLock { s in
            if let blocked = s.writeBlocked { throw SkillLibraryError.io(blocked) }
            let value: T
            let json: String
            do {
                value = try change(s.store)
                json = try s.store.toJson()
            } catch let error as SkillLibraryError {
                throw error
            } catch {
                throw SkillLibraryError(core: error)
            }
            try write(json)
            return value
        }
    }

    private func write(_ json: String) throws {
        do {
            try FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(json.utf8).write(to: fileURL, options: Self.writeOptions)
        } catch {
            throw SkillLibraryError.io("cannot write \(fileURL.lastPathComponent): \(error)")
        }
    }

    private static var writeOptions: Data.WritingOptions {
        #if canImport(Darwin)
        return [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
        #else
        return [.atomic]
        #endif
    }

    /// An id for a new skill (or workflow) from its name: lowercase ASCII letters and digits
    /// joined by `-`, at most 40 characters, `fallback` when nothing is left, made unique with `-2`, `-3`….
    static func makeID(from name: String, taken: Set<String>, fallback: String = "skill") -> String {
        var slug = ""
        for scalar in name.lowercased().unicodeScalars {
            if (scalar >= "a" && scalar <= "z") || (scalar >= "0" && scalar <= "9") {
                slug.unicodeScalars.append(scalar)
            } else if !slug.isEmpty && !slug.hasSuffix("-") {
                slug.append("-")
            }
        }
        while slug.hasSuffix("-") { slug.removeLast() }
        slug = String(slug.prefix(40))
        while slug.hasSuffix("-") { slug.removeLast() }
        if slug.isEmpty { slug = fallback }
        guard taken.contains(slug) else { return slug }
        var n = 2
        while taken.contains("\(slug)-\(n)") { n += 1 }
        return "\(slug)-\(n)"
    }
}

/// Skill files for the simulator tests (launch arguments seed them).
public enum SkillSeeds {
    /// A snapshot with one entry the core cannot load (a number where the name goes) and an
    /// unknown top-level field: two load issues, and the entry is kept.
    public static let damaged = #"{"schema_version":1,"skills":[{"id":"broken-entry","name":42}],"from_the_future":true}"#

    /// Two enabled Written skills sharing the phrase "network off": the matcher calls it
    /// ambiguous, so Ask offers a choice.
    public static let ambiguous = #"""
    {"schema_version":1,"skills":[
    {"id":"network-off-home","name":"Network off at home","summary":"","kind":"written","triggers":["network off"],"instructions":"Turn off Wi-Fi.","apps":[],"confine_to_apps":false,"params":[],"flow_keys":[],"enabled":true,"created_ms":0,"updated_ms":0,"runs":0,"last_run_ms":null,"tools":[]},
    {"id":"network-off-work","name":"Network off at work","summary":"","kind":"written","triggers":["network off"],"instructions":"Turn off Wi-Fi.","apps":[],"confine_to_apps":false,"params":[],"flow_keys":[],"enabled":true,"created_ms":0,"updated_ms":0,"runs":0,"last_run_ms":null,"tools":[]}
    ]}
    """#
}
