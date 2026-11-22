import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

private func tempDirectory() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-skills-\(UUID().uuidString)")
}

private func readJSON(_ url: URL) throws -> JSONValue {
    try JSONValue(parsing: String(decoding: try Data(contentsOf: url), as: UTF8.self))
}

/// What the core itself says about `skill`, to compare messages verbatim.
private func coreRefusal(_ skill: Skill) -> String? {
    do {
        _ = try JarvisSkills(json: "").upsert(skill: skill)
        return nil
    } catch {
        return CoreErrorMessage(error).message
    }
}

@Suite("SkillLibrary")
struct SkillLibraryTests {
    @Test("create, update, enable, remove; every change is on disk")
    func crud() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("skills.json")
        let library = SkillLibrary(fileURL: url, now: { Date(timeIntervalSince1970: 1_000) })
        #expect(library.skills.isEmpty)
        #expect(!library.hasLoadIssues)

        let created = try library.save(SkillDraft(name: "Lights out", triggers: ["lights out", "text {who} goodnight", ""],
                                                  instructions: "Use the Wi-Fi row.", apps: ["com.apple.Preferences"]))
        #expect(created.id == "lights-out")
        #expect(created.kind == .written)
        #expect(created.triggers == ["lights out", "text {who} goodnight"], "empty form rows are left out")
        #expect(created.params.map(\.name) == ["who"])
        #expect(created.params.first?.required == true)
        #expect(created.createdMs == 1_000_000)

        // A second skill with the same name gets its own id.
        let twin = try library.save(SkillDraft(name: "Lights out!", triggers: ["all lights out"]))
        #expect(twin.id == "lights-out-2")

        var draft = SkillDraft(skill: created)
        draft.summary = "Evening routine"
        draft.triggers = ["lights out"]
        let updated = try library.save(draft)
        #expect(updated.summary == "Evening routine")
        #expect(updated.kind == .written)
        #expect(updated.params.map(\.name) == ["who"], "a declared parameter without a slot is kept")

        try library.setEnabled(false, id: "lights-out")
        let reloaded = SkillLibrary(fileURL: url)
        #expect(reloaded.skills.map(\.id) == ["lights-out", "lights-out-2"])
        #expect(reloaded.skill(id: "lights-out")?.enabled == false)
        #expect(reloaded.skill(id: "lights-out")?.summary == "Evening routine")

        #expect(try reloaded.remove(id: "lights-out-2"))
        #expect(try !reloaded.remove(id: "lights-out-2"))
        #expect(SkillLibrary(fileURL: url).skills.map(\.id) == ["lights-out"])

        try reloaded.recordRun(id: "lights-out")
        #expect(SkillLibrary(fileURL: url).skill(id: "lights-out")?.runs == 1)
    }

    @Test("invalid skills: the core's message verbatim, routed to the field, nothing written")
    func invalid() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("skills.json")
        let library = SkillLibrary(fileURL: url)

        let expectedName = coreRefusal(Skill(id: "x", name: "", summary: "", kind: .written, triggers: ["hello"],
                                             instructions: "", apps: [], confineToApps: false, params: [], flowKeys: [],
                                             enabled: true, createdMs: 0, updatedMs: 0, runs: 0, lastRunMs: nil))
        #expect(expectedName != nil)
        do {
            try library.save(SkillDraft(name: "", triggers: ["hello"]))
            Issue.record("an empty name was accepted")
        } catch let error as SkillLibraryError {
            #expect(error == .invalidSkill(expectedName ?? ""))
            #expect(error.description == expectedName)
            #expect(!error.description.contains("JarvisError"), "the binding's wrapper leaked: \(error)")
            #expect(error.field == .name)
        }

        do {
            try library.save(SkillDraft(name: "Bad slot", triggers: ["text {Who Is}"]))
            Issue.record("a bad slot was accepted")
        } catch let error as SkillLibraryError {
            guard case .invalidSkill(let message) = error else {
                Issue.record("not an invalid-skill error: \(error)")
                return
            }
            #expect(message.contains("{Who Is}") || message.contains("Who Is"), "\(message)")
            #expect(error.field == .triggers, "\(message)")
        }

        do {
            try library.save(SkillDraft(name: "No apps", triggers: ["x"], apps: ["not a bundle id!"]))
        } catch let error as SkillLibraryError {
            #expect(error.field == .apps, "\(error)")
        }

        #expect(!FileManager.default.fileExists(atPath: url.path), "a refused skill was written")
        #expect(library.skills.isEmpty)
    }

    @Test("damaged entries and unknown fields are reported and survive every save")
    func loadIssuesSurvive() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("skills.json")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(SkillSeeds.damaged.utf8).write(to: url)

        let library = SkillLibrary(fileURL: url)
        #expect(library.loadIssues.count == 2, "\(library.loadIssues)")
        #expect(library.loadIssues.contains { $0.contains("broken-entry") })
        #expect(library.loadIssues.contains { $0.contains("from_the_future") })
        #expect(library.skills.isEmpty)

        try library.save(SkillDraft(name: "Lights out", triggers: ["lights out"]))
        let entries = try readJSON(url)["skills"]?.arrayValue ?? []
        #expect(entries.count == 2)
        let kept = entries.first { $0["id"]?.stringValue == "broken-entry" }
        #expect(kept?["name"]?.numberValue == 42, "the damaged entry was not written back unchanged")

        let again = SkillLibrary(fileURL: url)
        #expect(again.loadIssues.contains { $0.contains("broken-entry") })
        #expect(again.skills.map(\.id) == ["lights-out"])
        // Removing the damaged entry is the owner's choice, and then it is gone.
        #expect(try again.remove(id: "broken-entry"))
        #expect(!SkillLibrary(fileURL: url).hasLoadIssues)
    }

    @Test("a file the core refuses as a whole is set aside, never deleted")
    func setAside() throws {
        for content in ["not json at all", #"{"schema_version":99,"skills":[]}"#] {
            let dir = tempDirectory()
            defer { try? FileManager.default.removeItem(at: dir) }
            let url = dir.appendingPathComponent("skills.json")
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try Data(content.utf8).write(to: url)

            let library = SkillLibrary(fileURL: url, now: { Date(timeIntervalSince1970: 5) })
            #expect(library.skills.isEmpty)
            let issue = library.loadIssues.first ?? ""
            #expect(issue.contains("skills.json.corrupt-5000"), "\(issue)")
            let aside = dir.appendingPathComponent("skills.json.corrupt-5000")
            #expect(String(decoding: try Data(contentsOf: aside), as: UTF8.self) == content)
            try library.save(SkillDraft(name: "Fresh", triggers: ["fresh start"]))
            #expect(SkillLibrary(fileURL: url).skills.map(\.id) == ["fresh"])
        }
    }

    @Test("slots are read from triggers as typed")
    func slots() {
        #expect(SkillDraft.slots(in: "text {contact} {message}") == ["contact", "message"])
        #expect(SkillDraft.slots(in: "{a} and {a}") == ["a"])
        #expect(SkillDraft.slots(in: "no slots {unclosed") == [])
        #expect(SkillLibrary.makeID(from: "  Ünïcode — Name 2 ", taken: []) == "n-code-name-2")
        #expect(SkillLibrary.makeID(from: "!!!", taken: ["skill"]) == "skill-2")
    }

    @Test("CoreErrorMessage reads the case and message of any core error")
    func coreErrors() {
        do {
            _ = try JarvisSkills(json: "nope")
        } catch {
            let message = CoreErrorMessage(error)
            #expect(message.kind == "InvalidSkill")
            #expect(message.message.hasPrefix("invalid JSON"), "\(message.message)")
        }
        struct Other: Error {}
        #expect(CoreErrorMessage(Other()).kind == nil)
    }
}

@Suite("Skill routing and runs through the core")
struct SkillRoutingTests {
    private func library(in dir: URL) -> SkillLibrary {
        SkillLibrary(fileURL: dir.appendingPathComponent("skills.json"))
    }

    @Test("route: plain, exact run, missing values, ambiguous choice")
    func route() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let library = library(in: dir)
        try library.save(SkillDraft(name: "Lights out", triggers: ["lights out now"], instructions: "Use the Wi-Fi row."))
        try library.save(SkillDraft(name: "Text someone", triggers: ["text {who} {message}", "send a text"]))

        #expect(try library.route("what is the weather") == .plain)

        guard case .run(let candidate, let context) = try library.route("lights out now") else {
            Issue.record("an exact phrase did not run: \(try library.route("lights out now"))")
            return
        }
        #expect(candidate.skill.id == "lights-out")
        #expect(context.id == "lights-out")
        #expect(context.instructions == "Use the Wi-Fi row.")

        guard case .run(_, let filled) = try library.route("text Mom on my way") else {
            Issue.record("a fully filled slotted phrase did not run: \(try library.route("text Mom on my way"))")
            return
        }
        #expect(filled.args["who"] == "Mom", "\(filled.args)")

        guard case .askArguments(let needs) = try library.route("send a text") else {
            Issue.record("a phrase without values did not ask for them: \(try library.route("send a text"))")
            return
        }
        #expect(needs.missingParams.map(\.name) == ["who", "message"])
        let asked = try library.context(for: needs, values: ["who": "Ada", "message": "hello"])
        #expect(asked.args == ["who": "Ada", "message": "hello"])
        #expect(throws: SkillLibraryError.self) { try library.context(for: needs, values: ["who": "Ada"]) }

        // A skill turned off no longer matches, and a match made before is refused.
        try library.setEnabled(false, id: "lights-out")
        #expect(try library.route("lights out now") == .plain)
        #expect(throws: SkillLibraryError.self) { try library.context(for: candidate) }
    }

    @Test("the ambiguous seed asks the owner to choose")
    func ambiguousSeed() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(SkillSeeds.ambiguous.utf8).write(to: dir.appendingPathComponent("skills.json"))
        let library = library(in: dir)
        #expect(!library.hasLoadIssues, "\(library.loadIssues)")
        guard case .choose(let candidates) = try library.route("network off") else {
            Issue.record("not a choice: \(try library.route("network off"))")
            return
        }
        #expect(Set(candidates.map(\.id)) == ["network-off-home", "network-off-work"])
        // Choosing one runs it with its context.
        #expect(try library.context(for: candidates[0]).id == candidates[0].id)
    }

    @Test("exact match runs with the skill's guidance; just do what I said runs without")
    func runs() async throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let library = library(in: dir)
        try library.save(SkillDraft(name: "Wi-Fi off", triggers: ["turn off wi-fi"], instructions: "Prefer the Wi-Fi row on top."))
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        var settings = JarvisSettings()
        settings.firstUseApproval = false

        guard case .run(_, let context) = try library.route("Turn off Wi-Fi") else {
            Issue.record("no exact match")
            return
        }
        let prompts = Mutex<[String]>([])
        let live = Mutex(LiveRun(task: "Turn off Wi-Fi", maxSteps: 40, skillName: context.name))
        let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: settings, skill: context,
                                            human: HumanPromptBroker()) { e in
            if case .prompt(let p) = e { prompts.withLock { $0.append(p) } }
            live.withLock { $0.apply(e) }
        }
        #expect(live.withLock { $0.skillName } == "Wi-Fi off", "the run view lost the skill name")
        #expect(live.withLock { $0.status } == .done)
        guard case .finished(_, let result) = outcome else {
            Issue.record("not started: \(outcome)")
            return
        }
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        let first = prompts.withLock { $0.first } ?? ""
        #expect(first.contains("Prefer the Wi-Fi row on top."), "the skill's guidance is not in the prompt:\n\(first)")
        let record = TaskHistoryStore(fileURL: coordinator.paths.history).load().first
        #expect(record?.skillId == "wi-fi-off")
        #expect(record?.skillName == "Wi-Fi off")

        // "Just do what I said": no skill, no guidance (fresh memory so it is not a replay).
        try FileManager.default.removeItem(at: coordinator.memoryURL)
        let plainPrompts = Mutex<[String]>([])
        _ = await coordinator.run(task: "Turn off Wi-Fi", settings: settings, skill: nil, human: HumanPromptBroker()) { e in
            if case .prompt(let p) = e { plainPrompts.withLock { $0.append(p) } }
        }
        let plain = plainPrompts.withLock { $0.first } ?? ""
        #expect(!plain.isEmpty)
        #expect(!plain.contains("Prefer the Wi-Fi row on top."), "guidance leaked into a plain run:\n\(plain)")
        #expect(TaskHistoryStore(fileURL: coordinator.paths.history).load().first?.skillId == nil)
    }

    @Test("a skill confined to other apps refuses to act in the demo app")
    func confined() async throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let library = library(in: dir)
        try library.save(SkillDraft(name: "Notes only", triggers: ["turn off wi-fi"], apps: ["com.apple.mobilenotes"],
                                    confineToApps: true))
        guard case .run(_, let context) = try library.route("turn off wi-fi") else {
            Issue.record("no exact match")
            return
        }
        #expect(context.confineToApps)
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        var settings = JarvisSettings()
        settings.firstUseApproval = false
        let live = Mutex(LiveRun())
        let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: settings, skill: context,
                                            human: HumanPromptBroker()) { e in live.withLock { $0.apply(e) } }
        guard case .finished(_, let result) = outcome else {
            Issue.record("not started: \(outcome)")
            return
        }
        #expect(result.outcome.status != .success, "a confined skill acted outside its apps: \(result.outcome.summary)")
        let done = live.withLock { $0.steps.filter { $0.state == .done } }
        #expect(done.isEmpty, "steps ran in the demo app: \(done.map(\.label))")
    }

    @Test("a finished task's learned flow becomes a Learned skill that matches the task")
    func promote() async throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        var settings = JarvisSettings()
        settings.firstUseApproval = false
        _ = await coordinator.run(task: "Turn off Wi-Fi", settings: settings, human: HumanPromptBroker()) { _ in }
        let snapshot = await coordinator.memorySnapshot()
        let memory = await coordinator.memoryJSON()
        let flowIDs = snapshot.flows.map(\.id)
        #expect(SkillLibrary.learnedFlowID(forTask: "Delete my account", memoryJSON: memory, flowIDs: flowIDs) == nil)
        guard let flowID = SkillLibrary.learnedFlowID(forTask: "Turn off Wi-Fi", memoryJSON: memory, flowIDs: flowIDs) else {
            Issue.record("no flow for the task; flows: \(snapshot.flows.map(\.intentKey))")
            return
        }
        let library = library(in: dir)
        let skill = try library.promote(flowId: flowID, memoryJSON: memory, name: "Wi-Fi off")
        #expect(skill.kind == .learned)
        #expect(skill.name == "Wi-Fi off")
        #expect(!skill.flowKeys.isEmpty)
        if case .plain = try library.route("Turn off Wi-Fi") {
            Issue.record("the learned skill does not match its own task; triggers \(skill.triggers)")
        }
        // Promoting the same flow again is refused with the core's reason.
        #expect(throws: SkillLibraryError.self) { try library.promote(flowId: flowID, memoryJSON: memory, name: "Again") }
    }
}
