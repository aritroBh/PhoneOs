import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

private func tempDirectory() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-tests-\(UUID().uuidString)")
}

@Suite("JarvisSettings")
struct SettingsTests {
    @Test("round trip, and missing or unknown fields fall back to defaults")
    func codable() throws {
        var settings = JarvisSettings()
        settings.policyMode = .stepByStep
        settings.denyCategories = [.payment]
        settings.setAsksBefore(.systemSetting, false)
        settings.appTrust.append(AppTrust(bundleIdPrefix: "com.example.shop", name: "Shop", level: .trusted))
        let data = try JSONEncoder().encode(settings)
        #expect(try JSONDecoder().decode(JarvisSettings.self, from: data) == settings)

        let partial = #"{"policy_mode":"step_by_step","deny_categories":["payment","teleport"],"max_steps":"x","future":1}"#
        let decoded = try JSONDecoder().decode(JarvisSettings.self, from: Data(partial.utf8))
        #expect(decoded.policyMode == .stepByStep)
        #expect(decoded.denyCategories == [.payment], "unknown category names are skipped")
        #expect(decoded.maxSteps == 40, "a wrongly typed field takes the default")
        #expect(decoded.appTrust == JarvisSettings.defaultAppTrust)
    }

    @Test("apply maps onto the core's default policy and keeps its other defaults")
    func applyToPolicy() throws {
        var settings = JarvisSettings(policyMode: .stepByStep, firstUseApproval: false, denyCategories: [.purchase],
                                      maxSteps: 12, maxMinutes: 3)
        settings.setAsksBefore(.systemSetting, false)
        var policy = try defaultPolicy()
        let defaults = policy
        settings.apply(to: &policy)
        #expect(policy.mode == .stepByStep)
        #expect(!policy.firstUseApproval)
        #expect(policy.denyCategories.contains(.purchase))
        #expect(!policy.confirm.contains(.systemSetting))
        #expect(policy.confirm.contains(.payment))
        #expect(policy.maxSteps == 12)
        #expect(policy.maxDurationMs == 180_000)
        #expect(policy.maxLockedWaitMs == 30 * 60_000, "the default lock wait")
        #expect(policy.deniedApps == defaults.deniedApps, "Passwords and Wallet stay denied")
        #expect(policy.appRules.contains(AppRule(bundleIdPrefix: "com.apple.MobileSMS", level: .stepByStep)))
        #expect(policy.appRules.contains(AppRule(bundleIdPrefix: "com.apple.Passbook", level: .denied)))

        settings.maxSteps = 0
        settings.maxMinutes = 10_000
        settings.maxLockedWaitMinutes = 0
        settings.apply(to: &policy)
        #expect(policy.maxLockedWaitMs == 60_000)
        #expect(policy.maxSteps == 1)
        #expect(policy.maxDurationMs == 120 * 60_000)
    }

    @Test("trust lookup uses the longest prefix at dot boundaries")
    func trust() {
        var settings = JarvisSettings(appTrust: [
            AppTrust(bundleIdPrefix: "com.apple", name: "Apple", level: .guarded),
            AppTrust(bundleIdPrefix: "com.apple.MobileSMS", name: "Messages", level: .stepByStep),
        ])
        #expect(settings.trust(for: "com.apple.MobileSMS") == .stepByStep)
        #expect(settings.trust(for: "com.apple.Preferences") == .guarded)
        #expect(settings.trust(for: "com.applesauce") == nil)
        settings.appTrust = []
        #expect(settings.trust(for: "com.apple.Preferences") == nil)
    }

    @Test("the store returns defaults for a missing or corrupt file and saves atomically")
    func store() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = SettingsStore(fileURL: dir.appendingPathComponent("settings.json"))
        #expect(store.load() == JarvisSettings())
        var s = JarvisSettings()
        s.modelName = "m"
        try store.save(s)
        #expect(store.load() == s)
        try Data("not json".utf8).write(to: store.fileURL)
        #expect(store.load() == JarvisSettings())
    }

    @Test("category names for the owner")
    func titles() {
        #expect(riskCategoryTitle(.systemSetting) == "System setting")
        #expect(riskCategoryTitle(.firstUse) == "First time in this app")
        #expect(RiskCategoryOption(.none) == nil)
        #expect(RiskCategoryOption(.call) == .call)
        #expect(TrustLevelOption.stepByStep.explanation(app: "Messages").contains("every action in Messages"))
    }
}

@Suite("Task history and memory snapshot")
struct HistoryTests {
    @Test("records are kept newest first and bounded")
    func history() throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = TaskHistoryStore(fileURL: dir.appendingPathComponent("h.json"), capacity: 2)
        #expect(store.load().isEmpty)
        for i in 1...3 {
            try store.append(TaskRecord(id: "\(i)", task: "t\(i)", status: .success, summary: "", steps: i,
                                        modelCalls: 0, durationMs: 61_500, finishedAt: Date(timeIntervalSince1970: Double(i))))
        }
        let records = store.load()
        #expect(records.map(\.id) == ["3", "2"])
        #expect(records.first?.durationText == "1:01")
    }

    @Test("the snapshot reads log, flows and approved apps from real core memory")
    func snapshot() async throws {
        var h = Harness()
        h.firstUseApproval = true
        let (result, _) = try await h.run()
        #expect(result.outcome.status == .success)
        let snapshot = MemorySnapshot(json: try #require(h.memory.saves.first))
        #expect(snapshot.log.count == 1)
        #expect(snapshot.log.first?.task == "Turn off Wi-Fi")
        #expect(snapshot.log.first?.outcome == "success")
        #expect(snapshot.log.first?.steps == 2)
        #expect(snapshot.log.first?.apps == [settingsBundle])
        #expect(snapshot.flows.count == 1)
        #expect(snapshot.flows.first?.app == settingsBundle)
        #expect(snapshot.flows.first?.intentKey.isEmpty == false)
        #expect(snapshot.approvedApps == [settingsBundle])
        #expect(MemorySnapshot(json: "") == .empty)
        #expect(MemorySnapshot(json: "[1,2]") == .empty)
    }
}

@Suite("HumanPromptBroker")
struct BrokerTests {
    @Test("confirm and answer resolve through the pending prompt")
    func resolve() async throws {
        let seen = Mutex<[HumanPrompt?]>([])
        let broker = HumanPromptBroker { p in seen.withLock { $0.append(p) } }
        let request = ConfirmRequest(category: .delete, summary: "Delete?", app: "x")
        let decision = Task { try await broker.confirm(request) }
        try await waitUntil { broker.pending != nil }
        let prompt = try #require(broker.pending)
        #expect(prompt == .confirm(id: 1, request: request))
        #expect(!broker.reply(prompt.id, "wrong kind"), "a confirmation is not answered with text")
        #expect(broker.pending != nil)
        #expect(broker.approve(prompt.id))
        #expect(try await decision.value)
        #expect(broker.pending == nil)

        let answer = Task { try await broker.answer("Which?") }
        try await waitUntil { broker.pending != nil }
        let question = try #require(broker.pending)
        #expect(!broker.approve(question.id))
        #expect(broker.reply(question.id, "That one"))
        #expect(try await answer.value == "That one")
        #expect(seen.withLock { $0.count } == 4)
    }

    @Test("a newer prompt or cancelAll withdraws the pending one")
    func withdraw() async throws {
        let broker = HumanPromptBroker()
        let first = Task { try await broker.confirm(ConfirmRequest(category: .payment, summary: "Pay", app: "a")) }
        try await waitUntil { broker.pending != nil }
        let second = Task { try await broker.answer("Q?") }
        try await waitUntil { if case .question? = broker.pending { true } else { false } }
        await #expect(throws: HumanPromptError.withdrawn) { try await first.value }
        broker.cancelAll()
        await #expect(throws: HumanPromptError.withdrawn) { try await second.value }
        #expect(broker.pending == nil)
        #expect(!broker.decline(1))
    }
}

@Suite("Coordinator, live run state and connection probes")
struct CoordinatorTests {
    /// Approves every confirmation through the broker, as the sheet's Approve button does.
    private func autoApprovingBroker() -> HumanPromptBroker {
        let ref = BrokerRef()
        let broker = HumanPromptBroker { prompt in
            guard case .confirm(let id, _)? = prompt else { return }
            Task { ref.broker.withLock { $0 }?.approve(id) }
        }
        ref.broker.withLock { $0 = broker }
        return broker
    }

    /// Lets the broker's own callback reach the broker: a `Mutex` cannot be captured by a
    /// `sending` closure directly, a `Sendable` class holding it can.
    private final class BrokerRef: Sendable {
        let broker = Mutex<HumanPromptBroker?>(nil)
    }

    @Test("a demo run streams events, saves memory and history, and the second run replays")
    func demoRun() async throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        let events = Mutex<[RunEvent]>([])
        let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: autoApprovingBroker()) { e in
            events.withLock { $0.append(e) }
        }
        guard case .finished(let task, let result) = outcome else {
            Issue.record("not started: \(outcome)")
            return
        }
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(result.modelCalls == 3)

        var live = LiveRun(maxSteps: 40)
        for e in events.withLock({ $0 }) { live.apply(e) }
        #expect(live.status == .done)
        #expect(live.task == "Turn off Wi-Fi")
        #expect(live.steps.count == 2)
        #expect(live.steps.allSatisfy { $0.source == .model && $0.state == .done })
        #expect(live.steps.map(\.label) == [#"Tap "Wi-Fi""#, #"Tap "Wi-Fi""#])
        #expect(live.sight.contains { $0.role == "Switch" })
        #expect(live.chosenVerb == "done")
        #expect(live.caption.hasPrefix("DEMO SETTINGS · STEP"))
        #expect(live.maxSteps == 40)

        let history = TaskHistoryStore(fileURL: coordinator.paths.history).load()
        #expect(history.first?.id == task.id)
        #expect(history.first?.modelCalls == 3)
        #expect(history.first?.apps == [DemoPhone.bundleId])
        let snapshot = await coordinator.memorySnapshot()
        #expect(snapshot.flows.count == 1)
        #expect(snapshot.approvedApps == [DemoPhone.bundleId])
        #expect(!FileManager.default.fileExists(atPath: coordinator.paths.memory.path), "demo memory is separate")

        let replayEvents = Mutex<[RunEvent]>([])
        let replay = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: autoApprovingBroker()) { e in
            replayEvents.withLock { $0.append(e) }
        }
        guard case .finished(_, let second) = replay else {
            Issue.record("not started")
            return
        }
        #expect(second.modelCalls == 0)
        var replayed = LiveRun()
        for e in replayEvents.withLock({ $0 }) { replayed.apply(e) }
        #expect(replayed.steps.count == 2)
        #expect(replayed.steps.allSatisfy { $0.source == .replay })
        #expect(replayed.steps.first?.label == "Tap [1]")
    }

    @Test("needs-you while a confirmation waits; cancelling the run stops it")
    func cancelWhileWaiting() async throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        let live = Mutex(LiveRun())
        let broker = HumanPromptBroker()
        let run = Task {
            await coordinator.run(task: "Delete my account", settings: JarvisSettings(), human: broker) { e in
                live.withLock { $0.apply(e) }
            }
        }
        try await waitUntil { broker.pending != nil }
        #expect(live.withLock { $0.status } == .needsYou)
        run.cancel()
        guard case .finished(_, let result) = await run.value else {
            Issue.record("not started")
            return
        }
        #expect(result.endReason == .cancelled)
        #expect(live.withLock { $0.status } == .stopped)
    }

    @Test("a demo run on a locked phone waits, tells the owner, and finishes after the unlock")
    func demoLocked() async throws {
        let dir = tempDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let coordinator = JarvisCoordinator(
            paths: JarvisPaths(directory: dir), selection: .demo,
            configuration: DriverConfiguration(unlockPollInterval: .milliseconds(1)),
            demoDelay: .zero, demoLockedPolls: 3
        )
        let attention = RecordingAttention()
        let live = Mutex(LiveRun())
        let sawLocked = Mutex(false)
        let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: autoApprovingBroker(),
                                            attention: attention) { e in
            live.withLock { l in
                l.apply(e)
                if l.status == .locked { sawLocked.withLock { $0 = true } }
            }
        }
        guard case .finished(_, let result) = outcome else {
            Issue.record("not started")
            return
        }
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(sawLocked.withLock { $0 }, "the run view state went to Unlock to continue")
        #expect(live.withLock { $0.lockWaits } == 1)
        #expect(attention.log == ["await", "ended"])
        #expect(LiveRun.Status.locked.title == "Unlock to continue")
    }

    @Test("bad settings do not start a live run")
    func badSettings() async {
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: tempDirectory()), selection: .live)
        var settings = JarvisSettings()
        settings.modelBaseURL = "not a url"
        let outcome = await coordinator.run(task: "x", settings: settings, human: HumanPromptBroker()) { _ in }
        guard case .notStarted(let why) = outcome else {
            Issue.record("expected notStarted, got \(outcome)")
            return
        }
        #expect(why.contains("not a valid http(s) URL"))
    }

    @Test("probes: any HTTP answer is reachable; a closed port and a bad URL are not")
    func probes() async throws {
        let server = try StubHTTPServer { _ in .text("nope", status: 404) }
        defer { server.stop() }
        let probe = ConnectionProbe(timeout: 2)
        #expect(await probe.runner(server.baseURL.absoluteString) == .reachable("runner answered HTTP 404"))
        #expect(await probe.model(server.baseURL.absoluteString).isReachable)
        #expect(server.requests.map(\.path) == ["/whoami", "/v1/models"])
        #expect(!(await probe.runner("http://127.0.0.1:1")).isReachable)
        #expect(!(await probe.model("ftp://x")).isReachable)
    }
}

@Suite("FormCaptureServer")
struct FormCaptureServerTests {
    @Test("serves the form and records a submission")
    func roundTrip() async throws {
        let server = try FormCaptureServer()
        defer { server.stop() }
        let session = URLSession(configuration: .ephemeral)
        let (page, _) = try await session.data(from: server.baseURL)
        #expect(String(decoding: page, as: UTF8.self).contains(#"aria-label="Full name""#))

        var post = URLRequest(url: server.baseURL.appendingPathComponent("submit"))
        post.httpMethod = "POST"
        post.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        post.httpBody = Data("name=Ada+Lovelace&email=ada%40example.com".utf8)
        let (thanks, _) = try await session.data(for: post)
        #expect(String(decoding: thanks, as: UTF8.self).contains("Thanks, Ada Lovelace"))
        let submitted = await server.waitForSubmission(timeout: .seconds(2))
        #expect(submitted == ["name": "Ada Lovelace", "email": "ada@example.com"])
        #expect(server.log.count == 2)
        #expect(server.log.last?.hasPrefix("POST /submit") == true)
    }

    @Test("form decoding and HTML escaping")
    func decoding() {
        #expect(FormCaptureServer.parseForm("a=1&b=&c&d=x%20y+z") == ["a": "1", "b": "", "c": "", "d": "x y z"])
        #expect(FormCaptureServer.escapeHTML("<b>\"&") == "&lt;b&gt;&quot;&amp;")
    }
}
