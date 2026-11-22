import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

private func draft(_ name: String, _ steps: [StepDraft], trigger: TriggerDraft = TriggerDraft(),
                   onFailure: OnFailure = .stop) -> WorkflowDraft {
    WorkflowDraft(name: name, trigger: trigger, steps: steps, onFailure: onFailure)
}

@Suite("WorkflowLibrary", .timeLimit(.minutes(1)))
struct WorkflowLibraryTests {
    @Test("create, update, enable, record, remove; every change is on disk")
    func crud() throws {
        let dir = scratchDirectory("workflows")
        defer { try? FileManager.default.removeItem(at: dir) }
        let skills = SkillLibrary(fileURL: dir.appendingPathComponent("skills.json"))
        let url = dir.appendingPathComponent("workflows.json")
        let library = WorkflowLibrary(fileURL: url, skills: skills, now: { Date(timeIntervalSince1970: 2_000) })
        #expect(library.workflows.isEmpty)
        #expect(!library.hasLoadIssues)

        let created = try library.save(draft("Evening commute", [
            StepDraft(kind: .runTask, text: "Turn off Wi-Fi"),
            StepDraft(kind: .stopUnless, condition: .lastSucceeded),
            StepDraft(kind: .notify, text: "Home soon"),
        ], trigger: TriggerDraft(kind: .phrase, phrase: "heading home")))
        #expect(created.id == "evening-commute")
        #expect(created.steps.count == 3)
        #expect(created.createdMs == 2_000_000)
        #expect(created.trigger == .phrase(text: "heading home"))

        var edit = WorkflowDraft(workflow: created)
        edit.moveStep(at: 2, by: -1)
        #expect(edit.steps.map(\.kind) == [.runTask, .notify, .stopUnless])
        edit.steps.remove(at: 2)
        let updated = try library.save(edit)
        #expect(updated.steps.count == 2)
        #expect(updated.id == created.id)

        try library.setEnabled(false, id: created.id)
        try library.recordRun(id: created.id, outcome: WorkflowOutcome(status: .succeeded, stepsRun: 2, summary: "ok", failedStep: nil))
        let reloaded = WorkflowLibrary(fileURL: url, skills: skills)
        #expect(reloaded.workflow(id: created.id)?.enabled == false)
        #expect(reloaded.workflow(id: created.id)?.runs == 1)
        #expect(reloaded.workflow(id: created.id)?.lastOutcome?.summary == "ok")
        #expect(try reloaded.remove(id: created.id))
        #expect(WorkflowLibrary(fileURL: url, skills: skills).workflows.isEmpty)
    }

    @Test("the core's messages come back verbatim and point at the field; nothing is written")
    func invalid() throws {
        let dir = scratchDirectory("workflows")
        defer { try? FileManager.default.removeItem(at: dir) }
        let library = WorkflowLibrary(fileURL: dir.appendingPathComponent("workflows.json"),
                                      skills: SkillLibrary(fileURL: dir.appendingPathComponent("skills.json")))
        #expect(throws: WorkflowLibraryError.self) { try library.save(draft("", [StepDraft(kind: .notify, text: "x")])) }
        do {
            try library.save(draft("", [StepDraft(kind: .notify, text: "x")]))
        } catch let error as WorkflowLibraryError {
            #expect(error.field == .name, "\(error)")
            #expect(error.description.hasPrefix("name"), "\(error)")
        }
        do {
            try library.save(draft("Two", [StepDraft(kind: .notify, text: "fine"), StepDraft(kind: .notify, text: "  ")]))
            Issue.record("an empty notify text was accepted")
        } catch let error as WorkflowLibraryError {
            #expect(error.field == .step(1), "\(error)")
        }
        do {
            try library.save(draft("None", []))
            Issue.record("a workflow without steps was accepted")
        } catch let error as WorkflowLibraryError {
            #expect(error.field == .steps, "\(error)")
        }
        do {
            try library.save(draft("Sched", [StepDraft(kind: .notify, text: "x")],
                                   trigger: TriggerDraft(kind: .schedule, days: [], hour: 8, minute: 0)))
            Issue.record("a schedule without days was accepted")
        } catch let error as WorkflowLibraryError {
            #expect(error.field == .trigger, "\(error)")
        }
        #expect(!FileManager.default.fileExists(atPath: library.fileURL.path))
    }

    @Test("warnings name missing skills and missing values")
    func warnings() throws {
        let dir = scratchDirectory("workflows")
        defer { try? FileManager.default.removeItem(at: dir) }
        let skills = SkillLibrary(fileURL: dir.appendingPathComponent("skills.json"))
        try skills.save(SkillDraft(name: "Text someone", triggers: ["text {who} hello"]))
        let library = WorkflowLibrary(fileURL: dir.appendingPathComponent("workflows.json"), skills: skills)
        let w = draft("W", [StepDraft(kind: .runSkill, skillId: "nope"), StepDraft(kind: .runSkill, skillId: "text-someone")])
        let found = library.warnings(for: w)
        #expect(found.count == 2, "\(found)")
        #expect(found.first?.contains("nope") == true)
        var filled = w
        filled.steps = [StepDraft(kind: .runSkill, skillId: "text-someone", args: ["who": "Sam"])]
        #expect(library.warnings(for: filled).isEmpty)
    }

    @Test("a damaged entry is kept and reported; a corrupt file is set aside, never deleted")
    func damaged() throws {
        let dir = scratchDirectory("workflows")
        defer { try? FileManager.default.removeItem(at: dir) }
        let skills = SkillLibrary(fileURL: dir.appendingPathComponent("skills.json"))
        let url = dir.appendingPathComponent("workflows.json")
        // A valid store from the core, plus one entry the core cannot load.
        let good = try JarvisWorkflows(json: "")
        _ = try good.upsert(workflow: Workflow(id: "fine", name: "Fine", enabled: true, trigger: .manual,
                                               steps: [.notify(text: "hi")], onFailure: .stop, createdMs: 0, updatedMs: 0,
                                               runs: 0, lastRunMs: nil, lastOutcome: nil), nowMs: 1)
        var root = try JSONValue(parsing: try good.toJson())
        guard case .object(var object) = root, case .array(var list)? = object["workflows"] else {
            Issue.record("unexpected snapshot shape")
            return
        }
        list.append(.object(["id": .string("broken"), "name": .number(42)]))
        object["workflows"] = .array(list)
        root = .object(object)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try root.encoded().write(to: url)

        let library = WorkflowLibrary(fileURL: url, skills: skills)
        #expect(library.workflows.map(\.id) == ["fine"])
        #expect(library.loadIssues.contains { $0.contains("broken") }, "\(library.loadIssues)")
        try library.save(draft("Another", [StepDraft(kind: .notify, text: "x")]))
        let text = try String(contentsOf: url, encoding: .utf8)
        #expect(text.contains("broken"), "the damaged entry was dropped on save")

        try Data("not json".utf8).write(to: url)
        let fresh = WorkflowLibrary(fileURL: url, skills: skills)
        #expect(fresh.workflows.isEmpty)
        #expect(fresh.loadIssues.first?.contains("nothing was deleted") == true, "\(fresh.loadIssues)")
        let kept = try FileManager.default.contentsOfDirectory(atPath: dir.path).filter { $0.hasPrefix("workflows.json.corrupt-") }
        #expect(kept.count == 1)
    }

    @Test("Ask routing through resolveUtterance: workflow, skill, owner choice, plain")
    func resolve() throws {
        let dir = scratchDirectory("workflows")
        defer { try? FileManager.default.removeItem(at: dir) }
        let skills = SkillLibrary(fileURL: dir.appendingPathComponent("skills.json"))
        try skills.save(SkillDraft(name: "Lights out", triggers: ["lights out"]))
        try skills.save(SkillDraft(name: "Text someone", triggers: ["text {who} hello", "send a text"]))
        let library = WorkflowLibrary(fileURL: dir.appendingPathComponent("workflows.json"), skills: skills)
        try library.save(draft("Goodnight", [StepDraft(kind: .notify, text: "Night")],
                               trigger: TriggerDraft(kind: .phrase, phrase: "goodnight")))
        try library.save(draft("Lights routine", [StepDraft(kind: .notify, text: "x")],
                               trigger: TriggerDraft(kind: .phrase, phrase: "lights out")))

        guard case .runWorkflow(let w) = try library.resolve("Goodnight") else {
            Issue.record("a workflow phrase did not run the workflow")
            return
        }
        #expect(w.id == "goodnight")
        guard case .askArguments(let candidate) = try library.resolve("send a text") else {
            Issue.record("a slotted skill without its value did not ask for it: \(String(describing: try? library.resolve("send a text")))")
            return
        }
        #expect(candidate.skill.id == "text-someone")
        guard case .runSkill(_, let context) = try library.resolve("text Sam hello") else {
            Issue.record("an exact skill match did not run the skill")
            return
        }
        #expect(context.args["who"] == "Sam")
        guard case .choose(let workflows, let skillChoices) = try library.resolve("lights out") else {
            Issue.record("a workflow and an exact skill on the same phrase did not ask the owner")
            return
        }
        #expect(workflows.map(\.id) == ["lights-routine"])
        #expect(skillChoices.map(\.id) == ["lights-out"])
        #expect(try library.resolve("order a pizza") == .plain)
    }
}

@Suite("WorkflowRunner through the core (demo backend)", .timeLimit(.minutes(1)))
@MainActor
struct WorkflowRunnerTests {
    @Test("a multi-step run: task, conditions, a question, a notification and a wait; recorded and grouped")
    func multiStep() async throws {
        let h = AppHarness("wf-run")
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Evening", [
            StepDraft(kind: .runTask, text: "Turn off Wi-Fi"),
            StepDraft(kind: .stopUnless, condition: .lastSucceeded),
            StepDraft(kind: .askOwner, text: "Tell everyone?"),
            StepDraft(kind: .stopUnless, condition: .ownerSaidYes),
            StepDraft(kind: .notify, text: "Wi-Fi is off"),
            StepDraft(kind: .wait, waitSeconds: 1),
        ]))
        let started = h.runner.start(id: w.id)
        guard case .success(let task) = started else {
            Issue.record("refused: \(started)")
            return
        }
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .succeeded, "\(outcome)")
        #expect(outcome.stepsRun == 6)
        #expect(h.notifier.posted.withLock { $0 } == ["Wi-Fi is off"])
        #expect(h.owner.confirmCategories == [.firstUse], "the session still asked for first use")
        #expect(h.runner.progress?.states.allSatisfy { $0 == .done } == true, "\(String(describing: h.runner.progress?.states))")
        #expect(h.library.workflow(id: w.id)?.runs == 1)
        #expect(h.center.workflow == nil && h.center.session == nil)

        let history = h.history
        let entry = try #require(history.first { $0.isWorkflow == true })
        #expect(entry.task == "Evening")
        #expect(entry.status == .success)
        #expect(entry.sessionIds?.count == 1)
        let session = try #require(history.first { $0.id == entry.sessionIds?.first })
        #expect(session.isWorkflowSession)
        #expect(session.workflowRunId == entry.id)
        #expect(session.task == "Turn off Wi-Fi")
    }

    @Test("a risky step inside a workflow still asks you; declining stops the run")
    func declinedConfirmationStops() async throws {
        let h = AppHarness("wf-decline", rules: ScriptedOwner.Rules(approve: [.firstUse], decline: [.delete]))
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Clean up", [
            StepDraft(kind: .runTask, text: "Delete my account"),
            StepDraft(kind: .notify, text: "Should not be sent"),
        ], onFailure: .stop))
        guard case .success(let task) = h.runner.start(id: w.id) else {
            Issue.record("refused")
            return
        }
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(h.owner.confirmCategories.contains(.delete), "the delete confirmation was not asked: \(h.owner.confirmCategories)")
        #expect(outcome.status != .succeeded && outcome.status != .completedWithFailures, "\(outcome)")
        #expect(outcome.stepsRun == 1, "\(outcome)")
        #expect(h.notifier.posted.withLock { $0 }.isEmpty, "the run went on after the decline")
        #expect(h.runner.progress?.states.first == .failed)
    }

    @Test("an Ask-you answered no stops at the condition")
    func askOwnerNo() async throws {
        let h = AppHarness("wf-ask", rules: ScriptedOwner.Rules(yes: false))
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Ask", [
            StepDraft(kind: .askOwner, text: "Go on?"),
            StepDraft(kind: .stopUnless, condition: .ownerSaidYes),
            StepDraft(kind: .notify, text: "went on"),
        ]))
        guard case .success(let task) = h.runner.start(id: w.id) else {
            Issue.record("refused")
            return
        }
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .stopped, "\(outcome)")
        #expect(h.runner.progress?.states == [.done, .stoppedHere, .pending])
        #expect(h.notifier.posted.withLock { $0 }.isEmpty)
        #expect(h.history.first?.status == .cancelled, "a stop by condition reads Stopped in Activity")
    }

    @Test("cancel during a wait ends the run at once; a second start is refused while one runs")
    func cancelMidWaitAndSecondStart() async throws {
        let h = AppHarness("wf-wait")
        defer { h.cleanUp() }
        let long = try h.library.save(draft("Long wait", [StepDraft(kind: .wait, waitSeconds: 600), StepDraft(kind: .notify, text: "late")]))
        let other = try h.library.save(draft("Other", [StepDraft(kind: .notify, text: "x")]))
        guard case .success(let task) = h.runner.start(id: long.id) else {
            Issue.record("refused")
            return
        }
        try await eventually("the wait") {
            if case .waiting? = h.runner.progress?.activity { return true }
            return false
        }
        guard case .failure(.alreadyRunning(let name)) = h.runner.start(id: other.id) else {
            Issue.record("a second workflow started")
            return
        }
        #expect(name == "Long wait")
        guard case .failure(.workflowRunning) = h.center.start("Turn off Wi-Fi") else {
            Issue.record("an owner task started during a workflow")
            return
        }
        let before = ContinuousClock.now
        h.runner.cancel()
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .cancelled, "\(outcome)")
        #expect(ContinuousClock.now - before < .seconds(5))
        #expect(h.notifier.posted.withLock { $0 }.isEmpty)
        #expect(!h.center.isBusy)
    }

    // The two forms of the Wait step's sleep. The first one aborted the whole process on the
    // Mac (Swift 6.3.3) while the default was a closure in the init's signature.
    @Test("a workflow that only waits ends with the runner's own sleep")
    func waitOnlyOwnSleep() async throws {
        let h = AppHarness("wf-wait-own")
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Wait", [StepDraft(kind: .wait, waitSeconds: 1)]))
        guard case .success(let task) = h.runner.start(id: w.id) else {
            Issue.record("refused")
            return
        }
        let before = ContinuousClock.now
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .succeeded, "\(outcome)")
        #expect(ContinuousClock.now - before >= .milliseconds(900), "the wait was skipped")
    }

    @Test("an injected sleep gets the step's duration and replaces the real wait")
    func waitInjectedSleep() async throws {
        // The fake sleep moves the fake clock: with a real clock the core would (rightly) see
        // the wait as not over and ask for the rest again.
        let asked = Mutex<[Duration]>([])
        let clock = ManualClock()
        let h = AppHarness("wf-wait-injected", clock: clock, sleep: { duration in
            asked.withLock { $0.append(duration) }
            clock.advance(by: duration)
        })
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Wait", [StepDraft(kind: .wait, waitSeconds: 600), StepDraft(kind: .notify, text: "after")]))
        guard case .success(let task) = h.runner.start(id: w.id) else {
            Issue.record("refused")
            return
        }
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .succeeded, "\(outcome)")
        let durations = asked.withLock { $0 }
        #expect(durations.count == 1, "\(durations)")
        #expect(durations.first == .seconds(600), "\(durations)")
        #expect(h.notifier.posted.withLock { $0 } == ["after"])
    }

    @Test("cancel during a session stops the session and the run")
    func cancelMidSession() async throws {
        // The delete confirmation stays on screen (nobody answers it).
        let h = AppHarness("wf-session", rules: ScriptedOwner.Rules(approve: [.firstUse]))
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Risky", [StepDraft(kind: .runTask, text: "Delete my account"), StepDraft(kind: .notify, text: "x")]))
        guard case .success(let task) = h.runner.start(id: w.id) else {
            Issue.record("refused")
            return
        }
        try await eventually("the delete confirmation") { h.owner.confirmCategories.contains(.delete) }
        #expect(h.center.session?.origin.workflow?.workflowId == w.id)
        h.runner.cancel()
        let outcome = try await value(of: task, "the workflow run to end")
        #expect(outcome.status == .cancelled, "\(outcome)")
        #expect(h.center.session == nil && h.center.workflow == nil)
        #expect(h.center.broker.pending == nil, "the confirmation stayed on screen")
        #expect(h.history.first { $0.isWorkflowSession }?.status == .cancelled)
    }
}

@Suite("WorkflowScheduler", .timeLimit(.minutes(1)))
@MainActor
struct WorkflowSchedulerTests {
    /// Monday 2026-11-23, `hour:minute` UTC.
    private func at(_ hour: Int, _ minute: Int, dayOffset: Int = 0) -> LocalTime {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let date = calendar.date(from: DateComponents(year: 2026, month: 11, day: 23 + dayOffset, hour: hour, minute: minute))!
        return LocalTime(date: date, calendar: calendar)
    }

    @Test("local time maps Calendar weekdays to the core's")
    func localTime() {
        let t = at(7, 5)
        #expect(t.weekday == .mon)
        #expect(t.minuteOfDay == 425)
        #expect(at(0, 0, dayOffset: 6).weekday == .sun)
    }

    @Test("fires once per scheduled minute, catches up once inside the window, not outside it")
    func firesOnceAndCatchesUp() async throws {
        let h = AppHarness("wf-sched", libraryNow: { Date(timeIntervalSince1970: 0) })
        defer { h.cleanUp() }
        let stateURL = h.dir.appendingPathComponent("workflow-schedule.json")
        let w = try h.library.save(draft("Morning", [StepDraft(kind: .notify, text: "morning")],
                        trigger: TriggerDraft(kind: .schedule, days: [.mon], hour: 7, minute: 30)))
        let scheduler = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)

        #expect(scheduler.tick(at: at(7, 29), runner: h.runner) == nil, "fired early")
        #expect(scheduler.tick(at: at(7, 30), runner: h.runner) == w.id)
        try await eventually("the run") { !h.runner.isRunning }
        #expect(scheduler.tick(at: at(7, 30), runner: h.runner) == nil, "fired twice in one minute")
        #expect(scheduler.tick(at: at(7, 31), runner: h.runner) == nil, "fired again for the same scheduled minute")

        // A relaunch (fresh scheduler, same state file) inside the window does not fire again.
        let relaunched = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        #expect(relaunched.tick(at: at(7, 40), runner: h.runner) == nil)

        // Next Monday: the app opens 10 minutes late and catches up once.
        let nextWeek = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        #expect(nextWeek.tick(at: at(7, 40, dayOffset: 7), runner: h.runner) == w.id)
        try await eventually("the catch-up run") { !h.runner.isRunning }
        #expect(nextWeek.tick(at: at(7, 41, dayOffset: 7), runner: h.runner) == nil)

        // The week after, the app opens too late: no catch-up.
        #expect(nextWeek.tick(at: at(8, 0, dayOffset: 14), runner: h.runner) == nil)
        #expect(h.notifier.posted.withLock { $0 } == ["morning", "morning"])
        #expect(WorkflowLibrary.minutesUntilNextRun(of: try #require(h.library.workflow(id: w.id)), at: at(7, 0)) == 30)
    }

    @Test("a minute that ended before the workflow was saved is not caught up")
    func notBeforeSaved() async throws {
        let savedAt = Date(timeIntervalSince1970: Double(at(7, 31).epochMinute) * 60 + 10)
        let h = AppHarness("wf-sched2", libraryNow: { savedAt })
        defer { h.cleanUp() }
        _ = try h.library.save(draft("Morning", [StepDraft(kind: .notify, text: "m")],
                        trigger: TriggerDraft(kind: .schedule, days: [.mon], hour: 7, minute: 30)))
        let scheduler = WorkflowScheduler(library: h.library, catchUpMinutes: 15)
        #expect(scheduler.tick(at: at(7, 32), runner: h.runner) == nil, "caught up a minute that ended before it was saved")
        #expect(scheduler.due(at: at(7, 30, dayOffset: 7)).count == 1, "next week's minute is due")
    }

    @Test("while something runs, a due workflow waits and fires later inside the window")
    func waitsWhileBusy() async throws {
        let h = AppHarness("wf-sched3", libraryNow: { Date(timeIntervalSince1970: 0) })
        defer { h.cleanUp() }
        let w = try h.library.save(draft("Morning", [StepDraft(kind: .notify, text: "m")],
                                   trigger: TriggerDraft(kind: .schedule, days: [.mon], hour: 7, minute: 30)))
        let blocker = try h.library.save(draft("Blocker", [StepDraft(kind: .wait, waitSeconds: 600)]))
        guard case .success(let blocking) = h.runner.start(id: blocker.id) else {
            Issue.record("refused")
            return
        }
        let scheduler = WorkflowScheduler(library: h.library, catchUpMinutes: 15)
        #expect(scheduler.tick(at: at(7, 30), runner: h.runner) == nil, "started while another workflow ran")
        h.runner.cancel()
        _ = try await value(of: blocking, "the cancelled workflow to end")
        #expect(scheduler.tick(at: at(7, 36), runner: h.runner) == w.id, "did not fire once the phone was free")
        try await eventually("the run") { !h.runner.isRunning }
        #expect(h.notifier.posted.withLock { $0 } == ["m"])
    }
}
