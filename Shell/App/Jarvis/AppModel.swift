import Foundation
import SwiftUI
import JarvisCore
import JarvisKit

/// Launch arguments the app understands (UI tests pass them).
struct LaunchOptions {
    /// `-jarvis-playground`: show the Playground fixtures (QA target screens) and run tasks
    /// on the in-app demo backend (DemoPhone + scripted model), with its own data folder.
    var playground: Bool
    /// `-jarvis-reset`: start from empty settings, memory and history.
    var reset: Bool

    static let current = LaunchOptions(arguments: ProcessInfo.processInfo.arguments)

    init(arguments: [String]) {
        playground = arguments.contains("-jarvis-playground")
        reset = arguments.contains("-jarvis-reset")
    }
}

/// The app's state, on the main actor. A thin layer over JarvisKit: settings, the
/// coordinator, the live run, the pending owner prompt, history and connection status.
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
    /// Whether the run view is up (a run is going or its result is shown).
    @Published var showingRun = false

    let launch: LaunchOptions
    let coordinator: JarvisCoordinator
    private let settingsStore: SettingsStore
    private let historyStore: TaskHistoryStore
    private var runTask: Task<TaskRunOutcome, Never>?
    private lazy var broker: HumanPromptBroker = HumanPromptBroker { [weak self] _ in
        Task { @MainActor [weak self] in
            guard let self else { return }
            self.prompt = self.broker.pending
        }
    }

    init(launch: LaunchOptions) {
        self.launch = launch
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        let directory = base.appendingPathComponent(launch.playground ? "Jarvis-playground" : "Jarvis", isDirectory: true)
        if launch.reset { try? FileManager.default.removeItem(at: directory) }
        let paths = JarvisPaths(directory: directory)
        self.settingsStore = SettingsStore(fileURL: paths.settings)
        self.historyStore = TaskHistoryStore(fileURL: paths.history)
        self.settings = settingsStore.load()
        self.coordinator = JarvisCoordinator(paths: paths, selection: launch.playground ? .demo : .live)
        self.history = historyStore.load()
    }

    var isRunning: Bool { runTask != nil }

    /// Learned flows by their task key, for the Ask screen and Activity.
    var learnedFlowKeys: [String] { memory.flows.map(\.intentKey) }

    // MARK: - Running

    /// Starts `text` (ignored while a run is going).
    func start(_ text: String) {
        _ = startRun(text)
    }

    /// Runs `text` and returns the summary (App Intent).
    func runAndWait(_ text: String) async -> String {
        guard let task = startRun(text) else { return "Jarvis is already running a task." }
        switch await task.value {
        case .finished(_, let result): return result.outcome.summary
        case .notStarted(let why): return "Jarvis could not start: \(why)"
        }
    }

    private func startRun(_ text: String) -> Task<TaskRunOutcome, Never>? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, runTask == nil else { return nil }
        setupError = nil
        lastTask = trimmed
        live = LiveRun(task: trimmed, maxSteps: Int(settings.clampedMaxSteps))
        showingRun = true

        // Events arrive on the driver's executor; a stream keeps their order on the way to
        // the main actor (separate Tasks per event would not).
        let (events, sink) = AsyncStream.makeStream(of: RunEvent.self)
        let consumer = Task { @MainActor [weak self] in
            for await event in events { self?.live.apply(event) }
        }
        let coordinator = coordinator
        let settings = settings
        let broker = broker
        let task = Task { () -> TaskRunOutcome in
            let outcome = await coordinator.run(task: trimmed, settings: settings, human: broker) { _ = sink.yield($0) }
            sink.finish()
            await consumer.value
            return outcome
        }
        runTask = task
        Task { @MainActor [weak self] in
            let outcome = await task.value
            guard let self else { return }
            self.runTask = nil
            self.broker.cancelAll()
            if case .notStarted(let why) = outcome {
                self.setupError = why
                self.live.apply(.update(.finished(RunResult(
                    outcome: Outcome(status: .failed, summary: why, steps: 0, durationMs: 0),
                    endReason: .sessionFinished, memory: .unavailable(why), effectCount: 0, modelCalls: 0
                ))))
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

    // MARK: - Data

    func refresh() async {
        history = historyStore.load()
        memory = await coordinator.memorySnapshot()
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

    func setTrust(_ level: TrustLevelOption, for bundleIdPrefix: String) {
        guard let i = settings.appTrust.firstIndex(where: { $0.bundleIdPrefix == bundleIdPrefix }) else { return }
        settings.appTrust[i].level = level
    }

    func addApp(bundleId: String, name: String) {
        let id = bundleId.trimmingCharacters(in: .whitespaces)
        guard !id.isEmpty, !settings.appTrust.contains(where: { $0.bundleIdPrefix == id }) else { return }
        let display = name.trimmingCharacters(in: .whitespaces)
        settings.appTrust.append(AppTrust(bundleIdPrefix: id, name: display.isEmpty ? id : display, level: .guarded))
    }
}
