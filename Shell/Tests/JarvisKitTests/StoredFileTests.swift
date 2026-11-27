import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// Tests run as root here, so `chmod 000` does not make a file unreadable. "Unreadable" is
// simulated with a directory at the file's path (reading it fails, the path exists); a
// sentinel inside it shows that nothing replaced or removed it.

/// Puts a directory (with a sentinel file) at `url`, after moving any real file there to
/// `<url>.real`. `restore()` puts the real file back: the file "became readable".
private struct Unreadable {
    let url: URL
    var backup: URL { url.appendingPathExtension("real") }
    var sentinel: URL { url.appendingPathComponent("sentinel") }

    init(_ url: URL) throws {
        self.url = url
        let fm = FileManager.default
        try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        if fm.fileExists(atPath: url.path) { try fm.moveItem(at: url, to: backup) }
        try fm.createDirectory(at: url, withIntermediateDirectories: true)
        try Data("keep".utf8).write(to: sentinel)
    }

    /// The directory and its sentinel are still exactly there.
    var untouched: Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
            && (try? Data(contentsOf: sentinel)) == Data("keep".utf8)
    }

    func restore() throws {
        let fm = FileManager.default
        try fm.removeItem(at: url)
        if fm.fileExists(atPath: backup.path) { try fm.moveItem(at: backup, to: url) }
    }
}

private func makeDirectory(_ name: String) throws -> URL {
    let dir = scratchDirectory(name)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
}

private func files(in dir: URL) -> [String] {
    ((try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []).sorted()
}

private let fixedNow: @Sendable () -> Date = { Date(timeIntervalSince1970: 1_700_000_000) }
private let setAsideName = "corrupt-1700000000000"

private func ownersRules() -> JarvisSettings {
    var settings = JarvisSettings()
    settings.denyCategories = [.payment]
    settings.appTrust.append(AppTrust(bundleIdPrefix: "com.example.bank", name: "Bank", level: .denied))
    settings.firstUseApproval = true
    return settings
}

@Suite("Stored files: settings", .timeLimit(.minutes(1)))
struct SettingsFileTests {
    @Test("missing: the defaults, and the first save writes the file")
    func missing() throws {
        let dir = try makeDirectory("settings-missing")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"))
        #expect(store.read() == .missing)
        #expect(store.load() == JarvisSettings())
        try store.save(ownersRules())
        #expect(store.read() == .loaded(ownersRules()))
    }

    @Test("unreadable: reported, save refused, nothing on disk changes, readable again later")
    func unreadable() throws {
        let dir = try makeDirectory("settings-unreadable")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"))
        try store.save(ownersRules())
        let locked = try Unreadable(store.fileURL)

        guard case .unreadable = store.read() else {
            Issue.record("not reported as unreadable: \(store.read())")
            return
        }
        #expect(throws: StoredFileError.self) { try store.save(JarvisSettings()) }
        #expect(locked.untouched, "the unreadable file was replaced")
        #expect(files(in: dir) == ["settings.json", "settings.json.real"], "a file was set aside or added")

        try locked.restore()
        #expect(store.read() == .loaded(ownersRules()), "the owner's rules were not read back")
        var changed = ownersRules()
        changed.maxSteps = 7
        try store.save(changed)
        #expect(store.read() == .loaded(changed))
    }

    @Test("damaged: set aside with its bytes, reported with its new name, then saving works")
    func damaged() throws {
        let dir = try makeDirectory("settings-damaged")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"), now: fixedNow)
        let bytes = Data(#"{"deny_categories":["payment"],"#.utf8)
        try bytes.write(to: store.fileURL)

        guard case .damaged(_, let setAside?) = store.read() else {
            Issue.record("not set aside: \(store.read())")
            return
        }
        #expect(setAside.lastPathComponent == "settings.json.\(setAsideName)")
        #expect(try Data(contentsOf: setAside) == bytes, "the damaged bytes were not kept")
        #expect(files(in: dir) == ["settings.json.\(setAsideName)"])
        #expect(store.read() == .missing)

        // A second damaged file never overwrites the first one set aside.
        try Data("[]".utf8).write(to: store.fileURL)
        guard case .damaged(_, let second?) = store.read() else {
            Issue.record("the second damaged file was not set aside")
            return
        }
        #expect(second != setAside)
        #expect(try Data(contentsOf: setAside) == bytes)

        try store.save(ownersRules())
        #expect(store.read() == .loaded(ownersRules()))
    }

    @Test("the gate: unreadable refuses with plain words and recovers on refresh")
    @MainActor
    func gateUnreadable() throws {
        let dir = try makeDirectory("settings-gate")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"))
        try store.save(ownersRules())
        let locked = try Unreadable(store.fileURL)

        let gate = SettingsGate(store: store)
        #expect(gate.refusal == "Jarvis cannot read your rules yet. Unlock the phone and try again.")
        #expect(throws: StoredFileError.self) { try gate.save(JarvisSettings(maxSteps: 3)) }
        #expect(!gate.refresh(), "nothing changed while still unreadable")
        #expect(locked.untouched)

        try locked.restore()
        #expect(gate.refresh())
        #expect(gate.refusal == nil)
        #expect(gate.problem == nil)
        #expect(gate.settings == ownersRules())
    }

    @Test("the gate: damaged refuses until the owner acknowledges the review")
    @MainActor
    func gateDamaged() throws {
        let dir = try makeDirectory("settings-gate-damaged")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"), now: fixedNow)
        try Data("not json".utf8).write(to: store.fileURL)

        let gate = SettingsGate(store: store)
        let refusal = try #require(gate.refusal)
        #expect(refusal.contains("damaged and has been set aside"))
        #expect(refusal.contains("settings.json.\(setAsideName)"))
        #expect(refusal.contains("back to the defaults until you review them"))
        #expect(gate.settings == JarvisSettings())
        #expect(!gate.refresh(), "a refresh must not clear the damaged state")
        #expect(gate.refusal != nil)

        gate.acknowledgeReview()
        #expect(gate.refusal == nil)
        try gate.save(ownersRules())
        #expect(store.read() == .loaded(ownersRules()))
    }
}

/// A file with one safety field present but unusable (wrong type, `null`, a value this build
/// does not know, also inside a list); every other field is as the owner wrote it.
private let unusableSafetyFields: [String] = [
    #"{"deny_categories":"payment"}"#,
    #"{"deny_categories":["payment","teleport"]}"#,
    #"{"relaxed_categories":{"a":1}}"#,
    #"{"relaxed_categories":["send_message","beam"]}"#,
    #"{"app_trust":[{"bundleIdPrefix":"com.bank.app","name":"Bank","level":"denied"},{"bundleIdPrefix":"com.x","name":"X","level":"ask_twice"}]}"#,
    #"{"app_trust":{"com.bank.app":"denied"}}"#,
    #"{"app_trust":[{"bundleIdPrefix":"com.bank.app","level":"denied"}]}"#,
    #"{"policy_mode":"relaxed"}"#,
    #"{"policy_mode":1}"#,
    #"{"first_use_approval":"yes"}"#,
    #"{"first_use_approval":null}"#,
    #"{"remote_control_enabled":"off"}"#,
    #"{"max_steps":"40"}"#,
    #"{"max_minutes":2.5}"#,
    #"{"max_locked_wait_minutes":[30]}"#,
]

@Suite("Stored files: settings safety fields are strict", .timeLimit(.minutes(1)))
@MainActor
struct SettingsStrictnessTests {
    @Test("a safety field that is present but cannot be decoded makes the rules file damaged: set aside, refused until reviewed",
          arguments: unusableSafetyFields)
    func unusableSafetyField(json: String) throws {
        let dir = try makeDirectory("settings-strict")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"), now: fixedNow)
        let bytes = Data(json.utf8)
        try bytes.write(to: store.fileURL)

        guard case .damaged(_, let setAside?) = store.read() else {
            Issue.record("not reported as damaged: \(json)")
            return
        }
        #expect(try Data(contentsOf: setAside) == bytes, "the original bytes were not kept")

        // The gate sees the same damage: tasks are refused until the owner reviewed the rules.
        try bytes.write(to: store.fileURL)
        let gate = SettingsGate(store: store)
        guard case .damaged? = gate.problem else {
            Issue.record("the gate let \(json) through")
            return
        }
        #expect(gate.refusal != nil)
        #expect(!gate.refresh())
        #expect(gate.refusal != nil, "a refresh ended the review")
        gate.acknowledgeReview()
        #expect(gate.refusal == nil)
    }

    @Test("an unknown trust level never drops the owner's Denied apps or denied categories silently")
    func unknownTrustLevelIsReported() throws {
        let dir = scratchDirectory("probe-settings")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("settings.json")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let json = #"""
        {"deny_categories":"payment",
         "app_trust":[{"bundleIdPrefix":"com.bank.app","name":"Bank","level":"denied"},
                      {"bundleIdPrefix":"com.x","name":"X","level":"ask_twice"}]}
        """#
        try Data(json.utf8).write(to: url)
        let gate = SettingsGate(store: SettingsStore(fileURL: url))
        // Either the file is reported (damaged / needs review) or the owner's rules survive.
        let reported = gate.problem != nil
        #expect(reported || gate.settings.trust(for: "com.bank.app") == .denied,
                "the owner's Denied rule for com.bank.app vanished with no problem reported")
        #expect(reported || gate.settings.denyCategories.contains(.payment),
                "deny_categories was dropped silently")
    }

    @Test("a file without the safety keys still loads with the defaults (older files)")
    func missingKeysTakeDefaults() throws {
        let dir = try makeDirectory("settings-old")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"))
        try Data("{}".utf8).write(to: store.fileURL)
        #expect(store.read() == .loaded(JarvisSettings()))
        try Data(#"{"model_name":"m","future_field":[1,2]}"#.utf8).write(to: store.fileURL)
        #expect(store.read() == .loaded(JarvisSettings(modelName: "m")))
        let gate = SettingsGate(store: store)
        #expect(gate.problem == nil)
    }

    @Test("a lenient field with a wrong type still loads and keeps every safety field as written")
    func lenientFieldKeepsSafetyFields() throws {
        let dir = try makeDirectory("settings-lenient")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"))
        let json = #"""
        {"policy_mode":"step_by_step","first_use_approval":false,"relaxed_categories":["call"],
         "deny_categories":["payment","delete"],"max_steps":12,"max_minutes":3,"max_locked_wait_minutes":5,
         "app_trust":[{"bundleIdPrefix":"com.bank.app","name":"Bank","level":"denied"}],
         "remote_control_enabled":true,
         "model_base_url":42,"model_name":["x"],"model_context_tokens":"big","runner_base_url":false,
         "model_backend":"quantum","away_acknowledged":"maybe","away_proximity":3}
        """#
        try Data(json.utf8).write(to: store.fileURL)
        guard case .loaded(let s) = store.read() else {
            Issue.record("not loaded: \(store.read())")
            return
        }
        let d = JarvisSettings()
        #expect(s.policyMode == .stepByStep)
        #expect(!s.firstUseApproval)
        #expect(s.relaxedCategories == [.call])
        #expect(s.denyCategories == [.payment, .delete])
        #expect(s.maxSteps == 12 && s.maxMinutes == 3 && s.maxLockedWaitMinutes == 5)
        #expect(s.appTrust == [AppTrust(bundleIdPrefix: "com.bank.app", name: "Bank", level: .denied)])
        #expect(s.remoteControlEnabled)
        #expect(s.modelBaseURL == d.modelBaseURL && s.modelName == d.modelName && s.modelContextTokens == d.modelContextTokens)
        #expect(s.runnerBaseURL == d.runnerBaseURL && s.modelBackend == d.modelBackend)
        #expect(s.awayAcknowledged == d.awayAcknowledged && s.awayProximity == d.awayProximity)
    }
}

@Suite("Stored files: TaskCenter refuses while the rules cannot be used", .timeLimit(.minutes(1)))
@MainActor
struct SettingsRefusalTests {
    private func workflow(_ h: AppHarness) throws -> Workflow {
        try h.library.save(WorkflowDraft(name: "Notify", trigger: TriggerDraft(), steps: [StepDraft(kind: .notify, text: "hi")],
                                         onFailure: .stop))
    }

    @Test("unreadable: task and workflow starts are refused; after the unlock they run")
    func unreadable() async throws {
        let settingsURL = scratchDirectory("refusal-settings").appendingPathComponent("settings.json")
        defer { try? FileManager.default.removeItem(at: settingsURL.deletingLastPathComponent()) }
        let store = SettingsStore(fileURL: settingsURL)
        try store.save(ownersRules())
        let locked = try Unreadable(settingsURL)
        let gate = SettingsGate(store: store)
        let h = AppHarness("refusal-unreadable", settingsProblem: { gate.refusal })
        defer { h.cleanUp() }
        let w = try workflow(h)

        let expected = TaskCenter.Refusal.settingsUnavailable("Jarvis cannot read your rules yet. Unlock the phone and try again.")
        guard case .failure(let refusal) = h.center.start("Turn off Wi-Fi") else {
            Issue.record("a task started while the rules could not be read")
            return
        }
        #expect(refusal == expected)
        guard case .failure(.settingsUnavailable(let why)) = h.runner.start(id: w.id) else {
            Issue.record("a workflow started while the rules could not be read")
            return
        }
        #expect(why == expected.description)
        #expect(!h.center.isBusy)
        #expect(h.owner.seen.withLock { $0.isEmpty }, "something asked the owner")
        #expect(h.history.isEmpty)
        #expect(locked.untouched)

        try locked.restore()
        gate.refresh()
        guard case .success(let task) = h.center.start("Turn off Wi-Fi") else {
            Issue.record("refused after the rules became readable")
            return
        }
        let outcome = try await value(of: task, "the task")
        #expect(outcome.outcome.status == .success, "\(outcome)")
        guard case .success(let run) = h.runner.start(id: w.id) else {
            Issue.record("the workflow was refused after the rules became readable")
            return
        }
        #expect(try await value(of: run, "the workflow").status == .succeeded)
    }

    @Test("damaged: refused with the damaged-rules text until acknowledged")
    func damaged() async throws {
        let settingsURL = scratchDirectory("refusal-damaged").appendingPathComponent("settings.json")
        defer { try? FileManager.default.removeItem(at: settingsURL.deletingLastPathComponent()) }
        try FileManager.default.createDirectory(at: settingsURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{broken".utf8).write(to: settingsURL)
        let gate = SettingsGate(store: SettingsStore(fileURL: settingsURL))
        let h = AppHarness("refusal-damaged", settingsProblem: { gate.refusal })
        defer { h.cleanUp() }
        let w = try workflow(h)

        guard case .failure(.settingsUnavailable(let why)) = h.center.start("Turn off Wi-Fi") else {
            Issue.record("a task started before the owner reviewed the rules")
            return
        }
        #expect(why.contains("damaged"))
        guard case .failure(.settingsUnavailable) = h.runner.start(id: w.id) else {
            Issue.record("a workflow started before the owner reviewed the rules")
            return
        }

        gate.acknowledgeReview()
        guard case .success(let task) = h.center.start("Turn off Wi-Fi") else {
            Issue.record("refused after the owner reviewed the rules")
            return
        }
        #expect(try await value(of: task, "the task").outcome.status == .success)
    }

    @Test("a scheduled workflow waits while the rules cannot be read and fires once they can")
    func scheduledWaits() async throws {
        let problem = Mutex<String?>("Jarvis cannot read your rules yet. Unlock the phone and try again.")
        let h = AppHarness("refusal-schedule", libraryNow: { Date(timeIntervalSince1970: 0) },
                           settingsProblem: { problem.withLock { $0 } })
        defer { h.cleanUp() }
        let w = try h.library.save(WorkflowDraft(name: "Morning", trigger: TriggerDraft(kind: .schedule, days: [.mon], hour: 7, minute: 30),
                                                 steps: [StepDraft(kind: .notify, text: "m")], onFailure: .stop))
        let scheduler = WorkflowScheduler(library: h.library, catchUpMinutes: 15)
        #expect(scheduler.tick(at: mondayUTC(7, 30), runner: h.runner) == nil)
        problem.withLock { $0 = nil }
        #expect(scheduler.tick(at: mondayUTC(7, 31), runner: h.runner) == w.id, "the refused minute was not retried")
        try await eventually("the run") { !h.runner.isRunning }
        #expect(h.notifier.posted.withLock { $0 } == ["m"])
    }

    @Test("a paired device's task start is refused with the reason, not hung")
    func remoteRefused() async throws {
        let problem = Mutex<String?>(nil)
        let r = RemoteHarness("refusal-remote", settingsProblem: { problem.withLock { $0 } })
        defer { r.cleanUp() }
        let remote = try await r.pairDevice()
        problem.withLock { $0 = "Jarvis cannot read your rules yet. Unlock the phone and try again." }
        let mark = remote.messages.count
        try remote.send(.startTask(text: "Turn off Wi-Fi"))
        let finished = try await remote.waitForMessage(after: mark) { m in
            if case .finished = m { return true }
            return false
        }
        // The hub words the summary itself for a device without screen-text permission.
        guard case .finished(let status, _) = finished else { return }
        #expect(status == .failed)
        #expect(r.base.center.session == nil)
        #expect(r.base.history.isEmpty)
    }
}

/// Monday 2026-11-23, `hour:minute` UTC.
private func mondayUTC(_ hour: Int, _ minute: Int, dayOffset: Int = 0) -> LocalTime {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "UTC")!
    let date = calendar.date(from: DateComponents(year: 2026, month: 11, day: 23 + dayOffset, hour: hour, minute: minute))!
    return LocalTime(date: date, calendar: calendar)
}

private func date(of time: LocalTime) -> Date {
    Date(timeIntervalSince1970: Double(time.epochMinute) * 60)
}

@Suite("Stored files: workflow schedule state", .timeLimit(.minutes(1)))
@MainActor
struct ScheduleStateFileTests {
    private func morning(_ h: AppHarness) throws -> Workflow {
        try h.library.save(WorkflowDraft(name: "Morning", trigger: TriggerDraft(kind: .schedule, days: [.mon], hour: 7, minute: 30),
                                         steps: [StepDraft(kind: .notify, text: "morning")], onFailure: .stop))
    }

    @Test("unreadable: never fires a scheduled minute twice, never overwrites the state, works once readable")
    func unreadable() async throws {
        let h = AppHarness("sched-unreadable", libraryNow: { Date(timeIntervalSince1970: 0) })
        defer { h.cleanUp() }
        let stateURL = h.dir.appendingPathComponent("workflow-schedule.json")
        let w = try morning(h)
        let first = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        #expect(first.tick(at: mondayUTC(7, 30), runner: h.runner) == w.id)
        try await eventually("the run") { !h.runner.isRunning }
        let saved = try Data(contentsOf: stateURL)

        // Relaunch before the first unlock: the state file cannot be read.
        let locked = try Unreadable(stateURL)
        let relaunched = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        #expect(relaunched.loadIssue != nil)
        #expect(relaunched.due(at: mondayUTC(7, 35)).isEmpty)
        #expect(relaunched.tick(at: mondayUTC(7, 35), runner: h.runner) == nil, "fired again for the same scheduled minute")
        #expect(locked.untouched, "the state file was overwritten")

        // Unlocked: the file says the minute fired, so it does not fire again.
        try locked.restore()
        #expect(relaunched.tick(at: mondayUTC(7, 36), runner: h.runner) == nil, "fired twice after the unlock")
        #expect(relaunched.loadIssue == nil)
        #expect(try Data(contentsOf: stateURL) == saved)
        #expect(h.notifier.posted.withLock { $0 } == ["morning"])

        // And it works normally afterwards.
        #expect(relaunched.tick(at: mondayUTC(7, 30, dayOffset: 7), runner: h.runner) == w.id)
        try await eventually("next week's run") { !h.runner.isRunning }
        #expect(h.notifier.posted.withLock { $0 } == ["morning", "morning"])
    }

    @Test("unreadable: a minute marked meanwhile is merged into the file once it can be read")
    func markedWhileUnreadable() async throws {
        let h = AppHarness("sched-merge", libraryNow: { Date(timeIntervalSince1970: 0) })
        defer { h.cleanUp() }
        let stateURL = h.dir.appendingPathComponent("workflow-schedule.json")
        let w = try morning(h)
        let locked = try Unreadable(stateURL)
        let scheduler = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        scheduler.markFired(ScheduledFire(workflow: w, scheduledMinute: mondayUTC(7, 30).epochMinute))
        #expect(locked.untouched)
        try locked.restore()
        #expect(scheduler.due(at: mondayUTC(7, 31)).isEmpty, "the minute marked while unreadable was forgotten")
        let relaunched = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        #expect(relaunched.due(at: mondayUTC(7, 32)).isEmpty, "the merged state was not written")
    }

    @Test("damaged: set aside with its bytes; minutes before it was found are not caught up")
    func damaged() async throws {
        let h = AppHarness("sched-damaged", libraryNow: { Date(timeIntervalSince1970: 0) })
        defer { h.cleanUp() }
        let stateURL = h.dir.appendingPathComponent("workflow-schedule.json")
        let w = try morning(h)
        let bytes = Data("{\"fired\":".utf8)
        try FileManager.default.createDirectory(at: h.dir, withIntermediateDirectories: true)
        try bytes.write(to: stateURL)
        let foundAt = date(of: mondayUTC(7, 35))
        let scheduler = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15, now: { foundAt })
        let issue = try #require(scheduler.loadIssue)
        let keptName = "workflow-schedule.json.corrupt-\(Int(foundAt.timeIntervalSince1970 * 1_000))"
        #expect(issue.contains(keptName))
        #expect(try Data(contentsOf: h.dir.appendingPathComponent(keptName)) == bytes)
        #expect(scheduler.tick(at: mondayUTC(7, 36), runner: h.runner) == nil, "a minute that may have fired ran again")
        let relaunched = WorkflowScheduler(library: h.library, stateURL: stateURL, catchUpMinutes: 15)
        #expect(relaunched.due(at: mondayUTC(7, 37)).isEmpty, "the relaunch forgot the minutes before the damage")
        #expect(relaunched.tick(at: mondayUTC(7, 30, dayOffset: 7), runner: h.runner) == w.id)
        try await eventually("next week's run") { !h.runner.isRunning }
    }
}

@Suite("Stored files: task history", .timeLimit(.minutes(1)))
struct HistoryFileTests {
    private func record(_ id: String, _ seconds: Double) -> TaskRecord {
        TaskRecord(id: id, task: "t\(id)", status: .success, summary: "", steps: 1, modelCalls: 0, durationMs: 0,
                   finishedAt: Date(timeIntervalSince1970: seconds))
    }

    @Test("missing: empty, and append starts the file")
    func missing() throws {
        let dir = try makeDirectory("history-missing")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = TaskHistoryStore(fileURL: dir.appendingPathComponent("history.json"))
        #expect(store.read() == .missing)
        try store.append(record("1", 1))
        #expect(store.load().map(\.id) == ["1"])
    }

    @Test("unreadable: append refused and nothing on disk changes; the history is kept once readable")
    func unreadable() throws {
        let dir = try makeDirectory("history-unreadable")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = TaskHistoryStore(fileURL: dir.appendingPathComponent("history.json"))
        try store.append(record("1", 1))
        try store.append(record("2", 2))
        let locked = try Unreadable(store.fileURL)

        guard case .unreadable = store.read() else {
            Issue.record("not reported as unreadable")
            return
        }
        #expect(store.load().isEmpty)
        #expect(throws: StoredFileError.self) { try store.append(record("3", 3)) }
        #expect(locked.untouched, "the history was overwritten")

        try locked.restore()
        try store.append(record("4", 4))
        #expect(store.load().map(\.id) == ["4", "2", "1"], "the earlier history was lost")
    }

    @Test("damaged: set aside with its bytes, reported, and the list starts again")
    func damaged() throws {
        let dir = try makeDirectory("history-damaged")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = TaskHistoryStore(fileURL: dir.appendingPathComponent("history.json"), now: fixedNow)
        let bytes = Data(#"[{"id":"#.utf8)
        try bytes.write(to: store.fileURL)
        guard case .damaged(_, let setAside?) = store.read() else {
            Issue.record("not set aside")
            return
        }
        #expect(setAside.lastPathComponent == "history.json.\(setAsideName)")
        #expect(try Data(contentsOf: setAside) == bytes)
        #expect(try store.append(record("1", 1)).map(\.id) == ["1"])
        #expect(try Data(contentsOf: setAside) == bytes, "the set-aside file changed")
    }
}

@Suite("Stored files: a driver init failure sets the memory aside only when the core rejected it", .timeLimit(.minutes(1)))
struct DriverStartMemoryTests {
    private struct Other: Error {}

    @Test("an init error that is not about memory leaves a good memory file alone and the run does not start",
          arguments: [0, 1])
    func otherErrorLeavesMemory(kind: Int) async throws {
        let dir = try makeDirectory("start-other")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("memory.json")
        let bytes = Data(#"{"flows":[]}"#.utf8)
        try bytes.write(to: url)
        let calls = Mutex<[String]>([])
        let error: any Error = kind == 0 ? JarvisError.Internal(message: "boom") : Other()
        let result = await JarvisCoordinator.startDriver(memoryJSON: #"{"flows":[]}"#, memory: FileMemoryStore(fileURL: url)) { json -> Int in
            calls.withLock { $0.append(json) }
            throw error
        }
        guard case .failure(let refusal) = result else {
            Issue.record("started")
            return
        }
        #expect(refusal.text.hasPrefix("the core could not start a session"))
        #expect(refusal.text != JarvisCoordinator.memoryUnreadable)
        #expect(calls.withLock { $0 } == [#"{"flows":[]}"#], "retried with fresh memory")
        #expect(try Data(contentsOf: url) == bytes, "the memory file was changed")
        #expect(files(in: dir) == ["memory.json"], "the memory file was set aside")
    }

    @Test("the core's memory-rejected error sets the file aside and starts with fresh memory")
    func memoryRejectedQuarantines() async throws {
        let dir = try makeDirectory("start-memory")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("memory.json")
        let bytes = Data(#"{"schema":999}"#.utf8)
        try bytes.write(to: url)
        let calls = Mutex<[String]>([])
        let result = await JarvisCoordinator.startDriver(memoryJSON: #"{"schema":999}"#, memory: FileMemoryStore(fileURL: url, now: fixedNow)) { json -> Int in
            calls.withLock { $0.append(json) }
            if !json.isEmpty { throw JarvisError.InvalidMemory(message: "newer schema") }
            return 7
        }
        #expect((try? result.get()) == 7)
        #expect(calls.withLock { $0 } == [#"{"schema":999}"#, ""])
        #expect(files(in: dir) == ["memory.json.\(setAsideName)"])
        #expect(try Data(contentsOf: dir.appendingPathComponent("memory.json.\(setAsideName)")) == bytes)
    }
}

@Suite("Stored files: skills and workflows recover after the unlock", .timeLimit(.minutes(1)))
struct LibraryRecoveryTests {
    @Test("skills: unreadable at launch is empty and write-blocked; after a reload the skills are there and saving works")
    func skillsRecover() throws {
        let dir = try makeDirectory("skills-recover")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("skills.json")
        let original = SkillLibrary(fileURL: url)
        try original.save(SkillDraft(name: "Wifi off", summary: "s", triggers: ["wifi off"], instructions: "Turn off Wi-Fi"))
        let locked = try Unreadable(url)

        let library = SkillLibrary(fileURL: url)
        #expect(library.skills.isEmpty)
        #expect(library.isWriteBlocked)
        #expect(throws: SkillLibraryError.self) { try library.save(SkillDraft(name: "Other", summary: "s", triggers: ["other"], instructions: "x")) }
        #expect(!library.reloadIfBlocked(), "still unreadable")
        #expect(locked.untouched)

        try locked.restore()
        #expect(library.reloadIfBlocked())
        #expect(library.skills.map(\.name) == ["Wifi off"])
        #expect(!library.isWriteBlocked)
        try library.save(SkillDraft(name: "Other", summary: "s", triggers: ["other"], instructions: "x"))
        #expect(SkillLibrary(fileURL: url).skills.count == 2)
    }

    @Test("skills: a library that loaded is not re-read (in-memory state kept)")
    func skillsLoadedNotReread() throws {
        let dir = try makeDirectory("skills-noreload")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("skills.json")
        let library = SkillLibrary(fileURL: url)
        try library.save(SkillDraft(name: "Wifi off", summary: "s", triggers: ["wifi off"], instructions: "Turn off Wi-Fi"))
        // Another writer changes the file; a needless re-read would show its content.
        try Data(#"{"version":1,"skills":[]}"#.utf8).write(to: url)
        #expect(!library.reloadIfBlocked())
        #expect(library.skills.map(\.name) == ["Wifi off"])
    }

    @Test("workflows: unreadable at launch is empty and write-blocked; after a reload they are there and saving works; a loaded library is not re-read")
    func workflowsRecover() throws {
        let dir = try makeDirectory("workflows-recover")
        defer { try? FileManager.default.removeItem(at: dir) }
        let skills = SkillLibrary(fileURL: dir.appendingPathComponent("skills.json"))
        let url = dir.appendingPathComponent("workflows.json")
        let draft = { (name: String) in
            WorkflowDraft(name: name, trigger: TriggerDraft(), steps: [StepDraft(kind: .notify, text: "hi")], onFailure: .stop)
        }
        try WorkflowLibrary(fileURL: url, skills: skills).save(draft("Morning"))
        let locked = try Unreadable(url)

        let library = WorkflowLibrary(fileURL: url, skills: skills)
        #expect(library.workflows.isEmpty)
        #expect(library.isWriteBlocked)
        #expect(throws: WorkflowLibraryError.self) { try library.save(draft("Evening")) }
        #expect(!library.reloadIfBlocked())
        #expect(locked.untouched)

        try locked.restore()
        #expect(library.reloadIfBlocked())
        #expect(library.workflows.map(\.name) == ["Morning"])
        try library.save(draft("Evening"))
        #expect(WorkflowLibrary(fileURL: url, skills: skills).workflows.count == 2)

        // Loaded now: another reload does nothing and keeps what is in memory.
        try Data(#"{"version":1,"workflows":[]}"#.utf8).write(to: url)
        #expect(!library.reloadIfBlocked())
        #expect(library.workflows.count == 2)
    }
}

@Suite("Stored files: remote audit log", .timeLimit(.minutes(1)))
struct AuditFileTests {
    private func entry(_ text: String, _ seconds: Double) -> RemoteAuditRecord {
        RemoteAuditRecord(at: Date(timeIntervalSince1970: seconds), device: nil, text: text, isWarning: false)
    }

    @Test("missing: empty, and an entry starts the file")
    func missing() throws {
        let dir = try makeDirectory("audit-missing")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("remote-audit.json")
        let log = RemoteAuditLog(fileURL: url)
        #expect(log.entries.isEmpty)
        #expect(log.loadIssue == nil)
        log.append(entry("a", 1))
        #expect(RemoteAuditLog(fileURL: url).entries.map(\.text) == ["a"])
    }

    @Test("unreadable: entries kept in memory, file untouched; merged newest first and bounded once readable")
    func unreadable() throws {
        let dir = try makeDirectory("audit-unreadable")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("remote-audit.json")
        let earlier = RemoteAuditLog(fileURL: url, capacity: 3)
        for (i, text) in ["old1", "old2", "old3"].enumerated() { earlier.append(entry(text, Double(i + 1))) }
        let locked = try Unreadable(url)

        let log = RemoteAuditLog(fileURL: url, capacity: 3)
        #expect(log.entries.isEmpty)
        #expect(log.loadIssue != nil)
        log.append(entry("new1", 10))
        log.clear()
        log.append(entry("new2", 11))
        log.append(entry("new3", 12))
        #expect(log.entries.map(\.text) == ["new3", "new2"])
        #expect(locked.untouched, "the audit log was overwritten or removed")

        try locked.restore()
        log.append(entry("new4", 13))
        #expect(log.loadIssue == nil)
        #expect(log.entries.map(\.text) == ["new4", "new3", "new2"], "not newest first or not bounded")
        #expect(RemoteAuditLog(fileURL: url, capacity: 3).entries.map(\.text) == ["new4", "new3", "new2"])

        // With room, the file's entries come back behind the new ones.
        let wide = dir.appendingPathComponent("wide.json")
        let before = RemoteAuditLog(fileURL: wide)
        before.append(entry("old", 1))
        let wideLocked = try Unreadable(wide)
        let merged = RemoteAuditLog(fileURL: wide)
        merged.append(entry("new", 5))
        try wideLocked.restore()
        #expect(merged.entries.map(\.text) == ["new", "old"], "reading the entries did not merge the file")
        #expect(RemoteAuditLog(fileURL: wide).entries.map(\.text) == ["new", "old"])
    }

    @Test("damaged: set aside with its bytes and reported; new entries go to a new file")
    func damaged() throws {
        let dir = try makeDirectory("audit-damaged")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("remote-audit.json")
        let bytes = Data("nope".utf8)
        try bytes.write(to: url)
        let log = RemoteAuditLog(fileURL: url, now: fixedNow)
        let issue = try #require(log.loadIssue)
        #expect(issue.contains("remote-audit.json.\(setAsideName)"))
        #expect(try Data(contentsOf: dir.appendingPathComponent("remote-audit.json.\(setAsideName)")) == bytes)
        log.append(entry("a", 1))
        #expect(RemoteAuditLog(fileURL: url).entries.map(\.text) == ["a"])
    }
}

@Suite("Stored files: memory", .timeLimit(.minutes(1)))
struct MemoryFileTests {
    private final class BrokerRef: Sendable {
        let broker = Mutex<HumanPromptBroker?>(nil)
    }

    /// Approves every confirmation, as the sheet's Approve button does.
    private func approvingBroker() -> HumanPromptBroker {
        let ref = BrokerRef()
        let broker = HumanPromptBroker { prompt in
            guard case .confirm(let id, _)? = prompt else { return }
            Task { ref.broker.withLock { $0 }?.approve(id) }
        }
        ref.broker.withLock { $0 = broker }
        return broker
    }

    @Test("read reports the three cases")
    func cases() async throws {
        let dir = try makeDirectory("memory-cases")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = FileMemoryStore(fileURL: dir.appendingPathComponent("memory.json"), now: fixedNow)
        #expect(await store.read() == .missing)
        try await store.save("{}")
        #expect(await store.read() == .loaded("{}"))
        let locked = try Unreadable(store.fileURL)
        guard case .unreadable = await store.read() else {
            Issue.record("not reported as unreadable")
            return
        }
        await #expect(throws: MemoryStoreError.self) { try await store.load() }
        #expect(locked.untouched)
        try locked.restore()
        try Data("[1]".utf8).write(to: store.fileURL)
        guard case .damaged(_, let setAside?) = await store.read() else {
            Issue.record("not set aside")
            return
        }
        #expect(try Data(contentsOf: setAside) == Data("[1]".utf8))
    }

    @Test("a run whose memory cannot be read does not start and leaves the memory alone; it runs once readable")
    func runRefused() async throws {
        let dir = try makeDirectory("memory-run")
        defer { try? FileManager.default.removeItem(at: dir) }
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        let first = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: approvingBroker()) { _ in }
        guard case .finished = first else {
            Issue.record("the first run did not start: \(first)")
            return
        }
        let learned = try Data(contentsOf: coordinator.memoryURL)
        #expect(MemorySnapshot(json: String(decoding: learned, as: UTF8.self)).approvedApps == [DemoPhone.bundleId])
        let historyBefore = TaskHistoryStore(fileURL: coordinator.paths.history).load()

        let locked = try Unreadable(coordinator.memoryURL)
        let asked = Mutex(0)
        let broker = HumanPromptBroker { prompt in if prompt != nil { asked.withLock { $0 += 1 } } }
        let refused = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: broker) { _ in }
        #expect(refused == .notStarted(JarvisCoordinator.memoryUnreadable))
        #expect(asked.withLock { $0 } == 0, "the owner was asked during a run that must not start")
        #expect(locked.untouched, "the memory file was overwritten")
        #expect(TaskHistoryStore(fileURL: coordinator.paths.history).load() == historyBefore)

        try locked.restore()
        #expect(try Data(contentsOf: coordinator.memoryURL) == learned)
        let again = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: approvingBroker()) { _ in }
        guard case .finished(_, let result) = again else {
            Issue.record("did not run once readable: \(again)")
            return
        }
        #expect(result.modelCalls == 0, "the learned flow was not replayed: the memory was lost")
        let snapshot = await coordinator.memorySnapshot()
        #expect(snapshot.approvedApps == [DemoPhone.bundleId], "the first-use approval was lost")
    }
}
