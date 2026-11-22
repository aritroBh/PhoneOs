import Foundation
import SwiftUI
import UIKit
import JarvisCore
import JarvisKit

/// Launch arguments the app understands (UI tests pass them).
struct LaunchOptions {
    /// `-jarvis-playground`: show the Playground fixtures (QA target screens) and run tasks
    /// on the in-app demo backend (DemoPhone + scripted model), with its own data folder.
    var playground: Bool
    /// `-jarvis-reset`: start from empty settings, memory, history and skills.
    var reset: Bool
    /// `-jarvis-demo-locked` (with `-jarvis-playground`): the demo phone starts locked and
    /// unlocks itself after a few polls, to show "Unlock to continue".
    var demoLocked: Bool
    /// `-jarvis-seed-damaged-skills`: write a skills file with an entry the core cannot load
    /// (the load-issue banner test).
    var seedDamagedSkills: Bool
    /// `-jarvis-seed-ambiguous-skills`: write two skills sharing the phrase "network off"
    /// (the chooser test).
    var seedAmbiguousSkills: Bool

    static let current = LaunchOptions(arguments: ProcessInfo.processInfo.arguments)

    init(arguments: [String]) {
        playground = arguments.contains("-jarvis-playground")
        reset = arguments.contains("-jarvis-reset")
        demoLocked = arguments.contains("-jarvis-demo-locked")
        seedDamagedSkills = arguments.contains("-jarvis-seed-damaged-skills")
        seedAmbiguousSkills = arguments.contains("-jarvis-seed-ambiguous-skills")
    }
}

/// What Ask needs from the owner before a skill can run.
enum SkillChoice: Identifiable {
    /// The matcher could not pick: offer `candidates` and "Just do what I said".
    case choose(task: String, candidates: [SkillCandidate])
    /// Values for `params`. With a `candidate` it came from the owner's words (`task`);
    /// without, it is "Run now" from the skill's page.
    case arguments(task: String?, candidate: SkillCandidate?, skill: Skill, params: [SkillParam])

    var id: String {
        switch self {
        case .choose(let task, let candidates): "choose:\(task):\(candidates.map(\.id).joined(separator: ","))"
        case .arguments(let task, _, let skill, _): "arguments:\(skill.id):\(task ?? "")"
        }
    }
}

/// The app's state, on the main actor. A thin layer over JarvisKit: settings, the
/// coordinator, the live run, the pending owner prompt, history, skills, Away mode and
/// connection status.
@MainActor
final class AppModel: ObservableObject {
    /// One model for the scene and the App Intent.
    static let shared = AppModel(launch: .current)

    @Published var settings: JarvisSettings {
        didSet {
            guard settings != oldValue else { return }
            try? settingsStore.save(settings)
        }
    }
    @Published private(set) var live = LiveRun()
    @Published private(set) var prompt: HumanPrompt?
    @Published private(set) var history: [TaskRecord] = []
    @Published private(set) var memory: MemorySnapshot = .empty
    @Published private(set) var runnerStatus: ConnectionStatus = .unknown
    @Published private(set) var modelStatus: ConnectionStatus = .unknown
    /// Why the last request did not start, if it did not.
    @Published private(set) var setupError: String?
    @Published private(set) var lastTask: String?
    /// The skill context of the last run ("Run again" reuses it).
    @Published private(set) var lastSkill: SkillContext?
    /// Whether the run view is up (a run is going or its result is shown).
    @Published var showingRun = false
    /// A run is going (the composer and suggestions are disabled).
    @Published private(set) var isRunning = false

    // Skills
    @Published private(set) var skillList: [Skill] = []
    @Published private(set) var skillIssues: [String] = []
    /// The chooser or the values sheet on Ask.
    @Published var skillChoice: SkillChoice?
    /// Finished tasks whose learned flow can become a skill: task text → flow id.
    @Published private(set) var promotableFlows: [String: String] = [:]

    // Apps
    @Published private(set) var toolSpecs: [ToolSpec] = []

    // Away mode
    @Published private(set) var awayPhase: AwayModeController.Phase = .off

    let launch: LaunchOptions
    let coordinator: JarvisCoordinator
    let skills: SkillLibrary
    private let settingsStore: SettingsStore
    private let historyStore: TaskHistoryStore
    private let coreDeniedApps: [String]
    private var runTask: Task<TaskRunOutcome, Never>?
    private lazy var broker: HumanPromptBroker = HumanPromptBroker { [weak self] _ in
        Task { @MainActor [weak self] in
            guard let self else { return }
            self.prompt = self.broker.pending
        }
    }
    private lazy var away = AwayModeController(
        display: UIKitAwayDisplay(), acknowledged: settings.awayAcknowledged, proximity: settings.awayProximity
    )
    private var terminateObserver: NSObjectProtocol?
    /// The run chosen in the skill sheet, started when the sheet has gone.
    private var pendingRun: (task: String, skill: SkillContext?)?

    init(launch: LaunchOptions) {
        self.launch = launch
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        let directory = base.appendingPathComponent(launch.playground ? "Jarvis-playground" : "Jarvis", isDirectory: true)
        if launch.reset { try? FileManager.default.removeItem(at: directory) }
        let paths = JarvisPaths(directory: directory)
        Self.seedSkills(launch, at: paths.skills)
        self.settingsStore = SettingsStore(fileURL: paths.settings)
        self.historyStore = TaskHistoryStore(fileURL: paths.history)
        self.settings = settingsStore.load()
        // Playground mode backs the UI tests: lock polls and the lock watch run every
        // 0.5 s / 1 s and a lock wait ends after 20 s, so a wrong lock signal fails a test in
        // seconds. The demo phone stays locked for 12 polls (about 6 s) so the "Unlock to
        // continue" card is on screen long enough to be checked.
        self.coordinator = JarvisCoordinator(
            paths: paths, selection: launch.playground ? .demo : .live,
            configuration: launch.playground
                ? DriverConfiguration(humanTimeout: .seconds(30), unlockPollInterval: .milliseconds(500), lockWatchInterval: .seconds(1))
                : DriverConfiguration(),
            demoLockedPolls: launch.playground && launch.demoLocked ? 12 : 0,
            maxLockedWaitOverrideMs: launch.playground ? 20_000 : nil
        )
        self.skills = SkillLibrary(fileURL: paths.skills)
        self.coreDeniedApps = (try? defaultPolicy().deniedApps) ?? []
        self.history = historyStore.load()
        refreshSkills()
        // The brightness must come back even when the app is closed from the switcher.
        terminateObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.appWillTerminate() }
        }
    }

    private static func seedSkills(_ launch: LaunchOptions, at url: URL) {
        let seed: String? = launch.seedDamagedSkills ? SkillSeeds.damaged
            : launch.seedAmbiguousSkills ? SkillSeeds.ambiguous : nil
        guard let seed else { return }
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data(seed.utf8).write(to: url, options: [.atomic])
    }

    /// Learned flows by their task key, for the Ask screen and Activity.
    var learnedFlowKeys: [String] { memory.flows.map(\.intentKey) }

    // MARK: - Asking

    /// The owner's request from Ask: the core's matcher decides whether a skill runs it,
    /// whether to ask which one, or whether values are missing (ignored while a run is going).
    func ask(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, runTask == nil else { return }
        setupError = nil
        do {
            switch try skills.route(trimmed) {
            case .plain:
                _ = startRun(trimmed, skill: nil)
            case .run(_, let context):
                _ = startRun(trimmed, skill: context)
            case .askArguments(let candidate):
                skillChoice = .arguments(task: trimmed, candidate: candidate, skill: candidate.skill,
                                         params: candidate.missingParams)
            case .choose(let candidates):
                skillChoice = .choose(task: trimmed, candidates: candidates)
            }
        } catch {
            // The matcher failing must not stop the owner's task: run it as words.
            _ = startRun(trimmed, skill: nil)
            setupError = "Skills could not be matched (\(error)); the task ran without a skill."
        }
    }

    /// The owner picked `candidate` in the chooser.
    func choose(_ candidate: SkillCandidate, for task: String) {
        if candidate.missingParams.isEmpty {
            do {
                finishChoice(task, skill: try skills.context(for: candidate))
            } catch {
                skillChoice = nil
                setupError = String(describing: error)
            }
        } else {
            skillChoice = .arguments(task: task, candidate: candidate, skill: candidate.skill, params: candidate.missingParams)
        }
    }

    /// "Just do what I said": the words as a plain task.
    func runWithoutSkill(_ task: String) {
        finishChoice(task, skill: nil)
    }

    /// Closes the skill sheet; the run starts in `skillSheetDismissed`.
    private func finishChoice(_ task: String, skill: SkillContext?) {
        pendingRun = (task, skill)
        if skillChoice == nil {
            skillSheetDismissed()
        } else {
            skillChoice = nil
        }
    }

    /// The skill sheet is gone (chosen, or swiped away: then nothing runs).
    func skillSheetDismissed() {
        guard let run = pendingRun else { return }
        pendingRun = nil
        _ = startRun(run.task, skill: run.skill)
    }

    /// Values entered for a skill. Returns the core's reason when it refuses them (the
    /// sheet stays up and shows it).
    func submitArguments(_ choice: SkillChoice, values: [String: String]) -> String? {
        guard case .arguments(let task, let candidate, let skill, _) = choice else { return nil }
        do {
            let context: SkillContext
            let text: String
            if let candidate, let task {
                context = try skills.context(for: candidate, values: values)
                text = task
            } else {
                context = try skills.context(skillId: skill.id, values: values)
                text = try skills.taskText(skillId: skill.id, values: values)
            }
            finishChoice(text, skill: context)
            return nil
        } catch {
            return String(describing: error)
        }
    }

    /// "Run now" on a skill's page: asks for its parameters first when it has any.
    /// Returns the core's reason when it refuses (for example a disabled skill).
    func runNow(_ skill: Skill) -> String? {
        guard runTask == nil else { return "Jarvis is already running a task." }
        if !skill.params.isEmpty {
            skillChoice = .arguments(task: nil, candidate: nil, skill: skill, params: skill.params)
            return nil
        }
        return submitArguments(.arguments(task: nil, candidate: nil, skill: skill, params: []), values: [:])
    }

    /// Starts `text` as a plain task (suggestions use `ask`).
    func start(_ text: String) {
        ask(text)
    }

    /// "Run again" on the result: the same words under the same skill.
    func runAgain(_ text: String, skill: SkillContext?) {
        _ = startRun(text, skill: skill)
    }

    /// Runs `text` and returns the summary (App Intent). A skill is used only when the
    /// matcher found exactly one with everything filled; nothing can be asked from Siri.
    func runAndWait(_ text: String) async -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        var skill: SkillContext?
        if case .run(_, let context)? = try? skills.route(trimmed) { skill = context }
        guard let task = startRun(trimmed, skill: skill) else { return "Jarvis is already running a task." }
        switch await task.value {
        case .finished(_, let result): return result.outcome.summary
        case .notStarted(let why): return "Jarvis could not start: \(why)"
        }
    }

    private func startRun(_ text: String, skill: SkillContext?) -> Task<TaskRunOutcome, Never>? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, runTask == nil else { return nil }
        setupError = nil
        // A run (for example from Siri) brings up the run view, which Away mode would
        // cover: leave Away mode so the screen is put back and the run can be watched.
        if awayPhase != .off { exitAway() }
        lastTask = trimmed
        lastSkill = skill
        live = LiveRun(task: trimmed, maxSteps: Int(settings.clampedMaxSteps), skillName: skill?.name)
        showingRun = true

        // Events arrive on the driver's executor; a stream keeps their order on the way to
        // the main actor (separate Tasks per event would not).
        let (events, sink) = AsyncStream.makeStream(of: RunEvent.self)
        let consumer = Task { @MainActor [weak self] in
            for await event in events {
                // A lock abandoned the pending question or confirmation: take the sheet
                // away (the core asks again after the unlock).
                if case .update(.lockDetected) = event { self?.broker.cancelAll() }
                self?.live.apply(event)
            }
        }
        let coordinator = coordinator
        let settings = settings
        let broker = broker
        let task = Task { () -> TaskRunOutcome in
            let outcome = await coordinator.run(task: trimmed, settings: settings, skill: skill, human: broker,
                                                attention: UnlockNotifier()) { _ = sink.yield($0) }
            sink.finish()
            await consumer.value
            return outcome
        }
        runTask = task
        isRunning = true
        Task { @MainActor [weak self] in
            let outcome = await task.value
            guard let self else { return }
            self.runTask = nil
            self.isRunning = false
            self.broker.cancelAll()
            switch outcome {
            case .notStarted(let why):
                self.setupError = why
                self.live.apply(.update(.finished(RunResult(
                    outcome: Outcome(status: .failed, summary: why, steps: 0, durationMs: 0),
                    endReason: .sessionFinished, memory: .unavailable(why), effectCount: 0, modelCalls: 0
                ))))
            case .finished:
                if let skill { try? self.skills.recordRun(id: skill.id) }
            }
            await self.refresh()
        }
        return task
    }

    /// Stops the run (the driver delivers Cancel to the core at once).
    func stop() {
        runTask?.cancel()
        broker.cancelAll()
    }

    /// Leaves the finished run view.
    func dismissRun() {
        guard runTask == nil else { return }
        showingRun = false
        live = LiveRun()
    }

    // MARK: - Owner answers

    func approve(_ prompt: HumanPrompt) { broker.approve(prompt.id) }
    func decline(_ prompt: HumanPrompt) { broker.decline(prompt.id) }
    func reply(_ prompt: HumanPrompt, _ text: String) { broker.reply(prompt.id, text) }

    /// The sheet was swiped away: a confirmation is declined; a question stays unanswered
    /// (the driver stops the run rather than invent an answer).
    func dismissPrompt(_ prompt: HumanPrompt) {
        switch prompt {
        case .confirm: broker.decline(prompt.id)
        case .question: if broker.pending?.id == prompt.id { broker.cancelAll() }
        }
    }

    // MARK: - Skills

    func refreshSkills() {
        skillList = skills.skills
        skillIssues = skills.loadIssues
    }

    /// Saves a draft; the core's refusal comes back for the form to show.
    func saveSkill(_ draft: SkillDraft) -> Result<Skill, SkillLibraryError> {
        defer { refreshSkills() }
        do {
            return .success(try skills.save(draft))
        } catch let error as SkillLibraryError {
            return .failure(error)
        } catch {
            return .failure(.core(String(describing: error)))
        }
    }

    func setSkillEnabled(_ enabled: Bool, id: String) -> String? {
        defer { refreshSkills() }
        do {
            try skills.setEnabled(enabled, id: id)
            return nil
        } catch {
            return String(describing: error)
        }
    }

    func deleteSkill(id: String) -> String? {
        defer { refreshSkills() }
        do {
            _ = try skills.remove(id: id)
            return nil
        } catch {
            return String(describing: error)
        }
    }

    /// Makes the learned flow of `record`'s task a Learned skill named `name`. Returns the
    /// new skill, or the core's reason.
    func promote(_ record: TaskRecord, name: String) async -> Result<Skill, SkillLibraryError> {
        guard let flowID = promotableFlows[record.task] else {
            return .failure(.invalidSkill("no learned flow for \"\(record.task)\""))
        }
        let memoryJSON = await coordinator.memoryJSON()
        defer {
            refreshSkills()
            refreshPromotable(memoryJSON: memoryJSON)
        }
        do {
            return .success(try skills.promote(flowId: flowID, memoryJSON: memoryJSON, name: name))
        } catch let error as SkillLibraryError {
            return .failure(error)
        } catch {
            return .failure(.core(String(describing: error)))
        }
    }

    private func refreshPromotable(memoryJSON: String) {
        let promoted = Set(skillList.flatMap(\.flowKeys))
        let open = memory.flows.filter { !promoted.contains($0.intentKey) }.map(\.id)
        var result: [String: String] = [:]
        for record in history where record.status == .success && result[record.task] == nil {
            if let flowID = SkillLibrary.learnedFlowID(forTask: record.task, memoryJSON: memoryJSON, flowIDs: open) {
                result[record.task] = flowID
            }
        }
        promotableFlows = result
    }

    // MARK: - Apps

    /// The Apps list: ways in from the known apps and the registered tools.
    var appAccess: [AppAccess] {
        AppAccessCatalog.make(settings: settings, knownApps: coordinator.knownApps, tools: toolSpecs,
                              coreDeniedApps: coreDeniedApps)
    }

    /// Apps a skill can be limited to (the form's app list).
    var skillAppChoices: [AppAccess] { appAccess.filter(\.screen) }

    func name(ofApp bundleId: String) -> String {
        appAccess.first { $0.bundleId == bundleId }?.name ?? bundleId
    }

    // MARK: - Away mode

    /// From Control or Ask: the explanation the first time, else the black screen.
    func requestAway() {
        awayPhase = away.request()
    }

    func acknowledgeAway() {
        away.acknowledge()
        settings.awayAcknowledged = true
        awayPhase = away.phase
    }

    func dismissAwayExplanation() {
        away.dismissExplanation()
        awayPhase = away.phase
    }

    func exitAway() {
        away.exit()
        awayPhase = away.phase
    }

    func setAwayProximity(_ on: Bool) {
        away.setProximity(on)
        settings.awayProximity = on
    }

    /// Scene phase changes (RootView): the screen is put back whenever Jarvis is not active.
    func sceneBecameActive(_ active: Bool) {
        if active { away.appReturnedToForeground() } else { away.appLeftForeground() }
        awayPhase = away.phase
    }

    private func appWillTerminate() {
        away.appWillTerminate()
        awayPhase = away.phase
    }

    // MARK: - Data

    func refresh() async {
        history = historyStore.load()
        memory = await coordinator.memorySnapshot()
        toolSpecs = await coordinator.toolSpecs()
        refreshSkills()
        refreshPromotable(memoryJSON: await coordinator.memoryJSON())
    }

    func checkConnections() async {
        if launch.playground {
            runnerStatus = .reachable("demo phone (playground mode)")
            modelStatus = .reachable("scripted demo model (playground mode)")
            return
        }
        let probe = ConnectionProbe()
        async let runner = probe.runner(settings.runnerBaseURL)
        async let model = probe.model(settings.modelBaseURL)
        runnerStatus = await runner
        modelStatus = await model
    }

    /// Sets the trust level of an app; an app without a rule yet gets one.
    func setTrust(_ level: TrustLevelOption, for bundleIdPrefix: String, name: String? = nil) {
        if let i = settings.appTrust.firstIndex(where: { $0.bundleIdPrefix == bundleIdPrefix }) {
            settings.appTrust[i].level = level
        } else {
            settings.appTrust.append(AppTrust(bundleIdPrefix: bundleIdPrefix, name: name ?? bundleIdPrefix, level: level))
        }
    }

    func addApp(bundleId: String, name: String) {
        let id = bundleId.trimmingCharacters(in: .whitespaces)
        guard !id.isEmpty, !settings.appTrust.contains(where: { $0.bundleIdPrefix == id }) else { return }
        let display = name.trimmingCharacters(in: .whitespaces)
        settings.appTrust.append(AppTrust(bundleIdPrefix: id, name: display.isEmpty ? id : display, level: .guarded))
    }
}

/// The UIKit side of Away mode. The screen is the one of the app's window scene
/// (`UIScreen.main` is deprecated); without a scene, brightness reads 1 and writes nothing.
@MainActor
final class UIKitAwayDisplay: AwayDisplay {
    private var screen: UIScreen? {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first?.screen
    }

    var brightness: Double {
        get { screen.map { Double($0.brightness) } ?? 1 }
        set { screen?.brightness = CGFloat(min(max(newValue, 0), 1)) }
    }

    var isIdleTimerDisabled: Bool {
        get { UIApplication.shared.isIdleTimerDisabled }
        set { UIApplication.shared.isIdleTimerDisabled = newValue }
    }

    var isProximityMonitoringEnabled: Bool {
        get { UIDevice.current.isProximityMonitoringEnabled }
        set { UIDevice.current.isProximityMonitoringEnabled = newValue }
    }
}
