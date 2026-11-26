import Foundation
import SwiftUI
import UIKit
import JarvisCore
import JarvisKit

/// Launch arguments the app understands (UI tests pass them).
struct LaunchOptions {
    /// `-jarvis-playground`: show the Playground fixtures (QA target screens) and run tasks
    /// on the in-app demo backend (DemoPhone + scripted model), with its own data folder.
    /// Remote control then uses the in-memory transport and secret store (no network, no
    /// Keychain).
    var playground: Bool
    /// `-jarvis-reset`: start from empty settings, memory, history, skills and workflows.
    var reset: Bool
    /// `-jarvis-demo-locked` (with `-jarvis-playground`): the demo phone starts locked and
    /// unlocks itself after a few polls, to show "Unlock to continue".
    var demoLocked: Bool
    /// `-jarvis-seed-damaged-skills`: write a skills file with an entry the core cannot load.
    var seedDamagedSkills: Bool
    /// `-jarvis-seed-ambiguous-skills`: two skills sharing the phrase "network off".
    var seedAmbiguousSkills: Bool
    /// `-jarvis-seed-scheduled-workflow`: save `WorkflowSeeds.scheduled` (the list shows
    /// "Next run").
    var seedScheduledWorkflow: Bool
    /// `-jarvis-seed-remote-device` (with `-jarvis-playground`): turn remote control on and
    /// pair an in-process device ("Test Mac") over the in-memory transport.
    var seedRemoteDevice: Bool

    static let current = LaunchOptions(arguments: ProcessInfo.processInfo.arguments)

    init(arguments: [String]) {
        playground = arguments.contains("-jarvis-playground")
        reset = arguments.contains("-jarvis-reset")
        demoLocked = arguments.contains("-jarvis-demo-locked")
        seedDamagedSkills = arguments.contains("-jarvis-seed-damaged-skills")
        seedAmbiguousSkills = arguments.contains("-jarvis-seed-ambiguous-skills")
        seedScheduledWorkflow = arguments.contains("-jarvis-seed-scheduled-workflow")
        seedRemoteDevice = arguments.contains("-jarvis-seed-remote-device")
    }
}

/// What Ask needs from the owner before something can run.
enum SkillChoice: Identifiable {
    /// The core could not pick: offer `workflows`, `candidates` and "Just do what I said".
    case choose(task: String, candidates: [SkillCandidate], workflows: [Workflow])
    /// Values for `params`. With a `candidate` it came from the owner's words (`task`);
    /// without, it is "Run now" from the skill's page.
    case arguments(task: String?, candidate: SkillCandidate?, skill: Skill, params: [SkillParam])

    var id: String {
        switch self {
        case .choose(let task, let candidates, let workflows):
            "choose:\(task):\(candidates.map(\.id).joined(separator: ",")):\(workflows.map(\.id).joined(separator: ","))"
        case .arguments(let task, _, let skill, _): "arguments:\(skill.id):\(task ?? "")"
        }
    }
}

/// The app's state, on the main actor. A thin layer over JarvisKit: settings, the one task
/// entry (`TaskCenter`), the live run, the pending owner prompt, history, skills,
/// workflows, remote control, Away mode and connection status.
@MainActor
final class AppModel: ObservableObject {
    /// One model for the scene and the App Intents.
    static let shared = AppModel(launch: .current)

    @Published var settings: JarvisSettings {
        didSet {
            guard settings != oldValue else { return }
            // Refused while the rules file cannot be read: it is never overwritten. The owner
            // is told, because the file's value comes back the next time it is read.
            do {
                try settingsGate.save(settings)
            } catch {
                setupError = SettingsGate.unsavedMessage(error)
            }
        }
    }
    /// Why the owner's rules cannot be used yet (tasks are refused with these words).
    @Published private(set) var settingsProblem: String?
    /// The rules file was damaged and set aside: tasks wait for "I have reviewed my rules".
    @Published private(set) var settingsNeedReview = false
    @Published private(set) var live = LiveRun()
    /// The Live view: what Jarvis did, step by step, with a cursor (the current or last run).
    /// The core's viewer behind it holds the logic; `viewerTimeline` is republished only
    /// when an update changed something.
    private(set) var viewer = LiveViewModel()
    @Published private(set) var viewerTimeline = CoreViewerTimeline.empty
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
    /// Whether the run view is up (a task is going or its result is shown).
    @Published var showingRun = false
    /// Something runs (a task or a workflow): the composer and suggestions are disabled.
    @Published private(set) var isRunning = false

    // Skills
    @Published private(set) var skillList: [Skill] = []
    @Published private(set) var skillIssues: [String] = []
    /// The chooser or the values sheet on Ask.
    @Published var skillChoice: SkillChoice?
    /// Finished tasks whose learned flow can become a skill: task text → flow id.
    @Published private(set) var promotableFlows: [String: String] = [:]

    // Workflows
    @Published private(set) var workflowList: [Workflow] = []
    @Published private(set) var workflowIssues: [String] = []
    /// The workflow run (current or last), for the workflow run view.
    @Published private(set) var workflowProgress: WorkflowRunner.Progress?
    @Published var showingWorkflowRun = false

    // Remote control
    @Published private(set) var remoteStatus: RemoteControlService.Status = .off
    @Published private(set) var remoteDevices: [DeviceInfo] = []
    /// Devices paired with a code and waiting for the owner's Confirm here (they hold no
    /// permission until then).
    @Published private(set) var remotePending: [PendingDevice] = []
    /// Wrong codes tried against the open pairing offer (the pairing sheet warns).
    @Published private(set) var remoteWrongCodes: UInt32 = 0
    @Published private(set) var remotePairing: RemotePairingOffer?
    @Published private(set) var remoteError: String?
    @Published private(set) var remoteAudit: [RemoteAuditRecord] = []
    @Published private(set) var remoteConnected: Set<String> = []
    /// Devices that watch the Live view now ("watching now" in the device row).
    @Published private(set) var remoteWatching: Set<String> = []
    /// Where a controller reaches this phone (the connection details row).
    @Published private(set) var remoteAddresses: [RemotePhoneAddress] = []

    // Controlling another phone ("Control another phone")
    @Published private(set) var controllerState = RemoteControllerModel.State()
    /// The other phone's Live view timeline (republished when it changed).
    @Published private(set) var controllerTimeline = CoreViewerTimeline.empty

    // Apps
    @Published private(set) var toolSpecs: [ToolSpec] = []

    // Away mode
    @Published private(set) var awayPhase: AwayModeController.Phase = .off

    // Notifications ("Unlock to continue", Notify steps)
    @Published private(set) var notificationAccess: NotificationAccess = .unknown
    /// Permission was asked in this launch (it is asked once, while Jarvis is in front).
    private var askedForNotifications = false

    let launch: LaunchOptions
    let coordinator: JarvisCoordinator
    let skills: SkillLibrary
    let workflows: WorkflowLibrary
    private let paths: JarvisPaths
    private let settingsGate: SettingsGate
    private let historyStore: TaskHistoryStore
    private let coreDeniedApps: [String]
    /// The in-memory transport of playground mode (the seeded device connects through it).
    private let memoryTransport = InMemoryRemoteTransport()
    /// Jarvis's own runner (docs/JARVIS-RUNNER.md), used when `settings.runnerKind` is
    /// `.jarvis`: one client (it reconnects by itself), its key in the Keychain.
    private let ownRunner: JarvisRunnerClient
    private var seededRemote: InProcessRemote?
    private var schedulerLoop: Task<Void, Never>?

    private lazy var broker: HumanPromptBroker = HumanPromptBroker { [weak self] _ in
        Task { @MainActor [weak self] in
            guard let self else { return }
            self.prompt = self.broker.pending
        }
    }
    /// The one entry every task takes (Ask, Siri, Run now, workflow steps, paired devices).
    private(set) lazy var center = TaskCenter(coordinator: coordinator, broker: broker, attention: UnlockNotifier(),
                                              settings: { [weak self] in self?.settings ?? JarvisSettings() },
                                              settingsProblem: { [weak self] in self?.settingsGate.refusal })
    private(set) lazy var workflowRunner: WorkflowRunner = {
        let runner = WorkflowRunner(library: workflows, center: center,
                                    notifier: launch.playground ? SilentWorkflowNotifier() as any WorkflowNotifier : UserNotificationWorkflowNotifier())
        runner.onChange = { [weak self] in self?.workflowProgress = self?.workflowRunner.progress }
        return runner
    }()
    private lazy var scheduler = WorkflowScheduler(library: workflows, stateURL: paths.workflowSchedule)
    /// This device as the controller of another phone. Playground: the phone in this
    /// process over the in-memory transport (no network, no Keychain).
    private(set) lazy var controller: RemoteControllerModel = {
        let playground = launch.playground
        let model = RemoteControllerModel(
            secrets: playground ? InMemorySecretStore() as any SecretStore : KeychainSecretStore(service: Self.controllerKeychainService),
            random: SecureRandomSource(),
            dialer: playground ? InMemoryRemoteDialer(transport: memoryTransport) as any RemoteDialer : NetworkRemoteDialer(),
            deviceName: UIDevice.current.name
        )
        model.onChange = { [weak self] in self?.mirrorController() }
        return model
    }()
    static let controllerKeychainService = "dev.jarvis.app.controller"
    /// The controller's Live view never has real-screen frames (the stream carries none);
    /// an empty store of its own.
    let controllerFrames = ViewerFrameStore()
    private(set) lazy var remote: RemoteControlService = {
        let playground = launch.playground
        let transport = memoryTransport
        let service = RemoteControlService(
            secrets: playground ? InMemorySecretStore() as any SecretStore : KeychainSecretStore(),
            random: SecureRandomSource(),
            transport: { () -> any RemoteTransport in
                if playground { return transport }
                return NetworkRemoteTransport()
            },
            phoneName: UIDevice.current.name,
            center: center, runner: workflowRunner, desk: broker,
            audit: RemoteAuditLog(fileURL: paths.remoteAudit)
        )
        service.onChange = { [weak self] in self?.mirrorRemote() }
        return service
    }()
    private lazy var away = AwayModeController(
        display: UIKitAwayDisplay(), acknowledged: settings.awayAcknowledged, proximity: settings.awayProximity
    )
    private var terminateObserver: NSObjectProtocol?
    private var protectedDataObserver: NSObjectProtocol?
    /// The phone is locking: the approval sheet's screenshot goes.
    private var protectedDataLockObserver: NSObjectProtocol?
    /// The scene is active (the approval sheet draws the screenshot only then; it is
    /// captured and held whichever app is in front).
    @Published private(set) var appIsActive = true
    /// The start chosen in the Ask sheet, made when the sheet has gone.
    private var pendingStart: PendingStart?

    private enum PendingStart {
        case task(String, SkillContext?)
        case workflow(String)
    }

    init(launch: LaunchOptions) {
        self.launch = launch
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        let directory = base.appendingPathComponent(launch.playground ? "Jarvis-playground" : "Jarvis", isDirectory: true)
        if launch.reset {
            try? FileManager.default.removeItem(at: directory)
            // The paired devices live in the Keychain, not in the folder.
            if !launch.playground {
                try? KeychainSecretStore().delete(RemoteControlService.secretKey)
                try? KeychainSecretStore(service: Self.controllerKeychainService).delete(RemoteControllerModel.secretKey)
            }
        }
        let paths = JarvisPaths(directory: directory)
        self.paths = paths
        Self.seedSkills(launch, at: paths.skills)
        let settingsGate = SettingsGate(store: SettingsStore(fileURL: paths.settings))
        self.settingsGate = settingsGate
        // Jarvis's own runner hands its key over in the launch environment when it starts
        // (candidate R2, docs/JARVIS-RUNNER.md); kept in the Keychain, never logged.
        let ownRunnerKeys = ProvisionedRunnerKeySource(store: KeychainSecretStore(service: "dev.jarvis.app.runner"))
        var runnerKeyProblem: String?
        if !launch.playground {
            do {
                try ownRunnerKeys.adopt(environment: ProcessInfo.processInfo.environment)
            } catch {
                // The value is never repeated; the stored key (if any) stays.
                runnerKeyProblem = "Jarvis's runner passed a key this build cannot use (\(error))."
            }
        }
        let ownRunner = JarvisRunnerClient(connector: NWRunnerConnector(), keySource: ownRunnerKeys, random: SecureRandomSource())
        self.ownRunner = ownRunner
        self.historyStore = TaskHistoryStore(fileURL: paths.history)
        self.settings = settingsGate.settings
        // Playground mode backs the UI tests: lock polls and the lock watch run every
        // 0.5 s / 1 s and a lock wait ends after 20 s, so a wrong lock signal fails a test in
        // seconds. The demo phone stays locked for 12 polls (about 6 s) so the "Unlock to
        // continue" card is on screen long enough to be checked.
        // An unanswered question is withdrawn (and counts as declined) after 120 s here, 600 s
        // in the app proper. It was 30 s: on a slow simulator a UI test's own checks of the
        // sheet (whole-tree queries of 10-30 s each, runs 115 and 117) outlasted it and the
        // run ended "the owner declined" before the test tapped Approve.
        self.coordinator = JarvisCoordinator(
            paths: paths, selection: launch.playground ? .demo : .live,
            // Seventeenth review: Jarvis's own bundle id is an OS fact, never a literal.
            // Twenty-fifth review: the exact list of Jarvis's own bundles built from it.
            agentBundleIds: JarvisOwnBundles.ids(main: Bundle.main.bundleIdentifier ?? "dev.jarvis.app"),
            configuration: launch.playground
                ? DriverConfiguration(humanTimeout: .seconds(120), unlockPollInterval: .milliseconds(500), lockWatchInterval: .seconds(1))
                : DriverConfiguration(),
            demoLockedPolls: launch.playground && launch.demoLocked ? 12 : 0,
            maxLockedWaitOverrideMs: launch.playground ? 20_000 : nil,
            // No key on this phone: the run does not start (never the text runner instead).
            jarvisRunner: { () -> (any PhoneRunner)? in
                guard (try? ownRunnerKeys.runnerKey()) != nil else { return nil }
                return ownRunner
            }
        )
        let skills = SkillLibrary(fileURL: paths.skills)
        self.skills = skills
        let workflowLibrary = WorkflowLibrary(fileURL: paths.workflows, skills: skills)
        if launch.seedScheduledWorkflow { _ = try? workflowLibrary.save(WorkflowSeeds.scheduled) }
        self.workflows = workflowLibrary
        self.coreDeniedApps = (try? defaultPolicy().deniedApps) ?? []
        self.history = historyStore.load()
        center.observe { [weak self] event in self?.taskEvent(event) }
        refreshSkills()
        refreshWorkflows()
        // The brightness must come back even when the app is closed from the switcher.
        terminateObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.appWillTerminate() }
        }
        // The first unlock after a restart: the rules can be read now.
        protectedDataObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.protectedDataDidBecomeAvailableNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                // Unlocked again: the approval picture may be captured again.
                self?.coordinator.approvalFrames.isPhoneUnlocked = true
                self?.reloadSettings()
            }
        }
        protectedDataLockObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.protectedDataWillBecomeUnavailableNotification, object: nil, queue: .main
        ) { [weak self] _ in
            // The phone is locking: the approval picture goes now, and none is kept until
            // it is unlocked again.
            MainActor.assumeIsolated { self?.coordinator.approvalFrames.isPhoneUnlocked = false }
        }
        mirrorSettingsProblem()
        if launch.playground && launch.seedRemoteDevice {
            settings.remoteControlEnabled = true
        }
        if settings.remoteControlEnabled { remote.setEnabled(true) }
        if let runnerKeyProblem { setupError = runnerKeyProblem }
        if launch.playground && launch.seedRemoteDevice { seedRemoteDevice() }
        refreshNotificationAccess()
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

    /// The owner's request from Ask: `resolveUtterance` decides whether a workflow or a
    /// skill runs it, whether to ask which one, or whether values are missing (ignored while
    /// something runs).
    func ask(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !center.isBusy else { return }
        setupError = nil
        do {
            switch try workflows.resolve(trimmed) {
            case .plain:
                startRun(trimmed, skill: nil)
            case .runWorkflow(let workflow):
                startWorkflow(workflow.id)
            case .runSkill(_, let context):
                startRun(trimmed, skill: context)
            case .askArguments(let candidate):
                skillChoice = .arguments(task: trimmed, candidate: candidate, skill: candidate.skill,
                                         params: candidate.missingParams)
            case .choose(let workflows, let candidates):
                skillChoice = .choose(task: trimmed, candidates: candidates, workflows: workflows)
            }
        } catch {
            // The matcher failing must not stop the owner's task: run it as words.
            startRun(trimmed, skill: nil)
            setupError = "Skills and workflows could not be matched (\(error)); the task ran as you said it."
        }
    }

    /// The owner picked `candidate` in the chooser.
    func choose(_ candidate: SkillCandidate, for task: String) {
        if candidate.missingParams.isEmpty {
            do {
                finishChoice(.task(task, try skills.context(for: candidate)))
            } catch {
                skillChoice = nil
                setupError = String(describing: error)
            }
        } else {
            skillChoice = .arguments(task: task, candidate: candidate, skill: candidate.skill, params: candidate.missingParams)
        }
    }

    /// The owner picked a workflow in the chooser.
    func choose(workflow: Workflow) {
        finishChoice(.workflow(workflow.id))
    }

    /// "Just do what I said": the words as a plain task.
    func runWithoutSkill(_ task: String) {
        finishChoice(.task(task, nil))
    }

    /// Closes the sheet; the start happens in `skillSheetDismissed`.
    private func finishChoice(_ start: PendingStart) {
        pendingStart = start
        if skillChoice == nil {
            skillSheetDismissed()
        } else {
            skillChoice = nil
        }
    }

    /// The sheet is gone (chosen, or swiped away: then nothing runs).
    func skillSheetDismissed() {
        guard let start = pendingStart else { return }
        pendingStart = nil
        switch start {
        case .task(let text, let skill): startRun(text, skill: skill)
        case .workflow(let id): startWorkflow(id)
        }
    }

    /// Values entered for a skill. Returns the core's reason when it refuses them.
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
            finishChoice(.task(text, context))
            return nil
        } catch {
            return String(describing: error)
        }
    }

    /// "Run now" on a skill's page: asks for its parameters first when it has any.
    func runNow(_ skill: Skill) -> String? {
        guard !center.isBusy else { return TaskCenter.Refusal.busy.description }
        if !skill.params.isEmpty {
            skillChoice = .arguments(task: nil, candidate: nil, skill: skill, params: skill.params)
            return nil
        }
        return submitArguments(.arguments(task: nil, candidate: nil, skill: skill, params: []), values: [:])
    }

    func start(_ text: String) {
        ask(text)
    }

    /// "Run again" on the result: the same words under the same skill.
    func runAgain(_ text: String, skill: SkillContext?) {
        startRun(text, skill: skill)
    }

    /// Runs `text` and returns the summary (Ask Jarvis intent). A workflow or a skill is
    /// used only when the core picks exactly one with everything filled; nothing can be
    /// asked from Siri. Waits at most `limit`: a longer task keeps running and the answer
    /// says so.
    func runAndWait(_ text: String, limit: Duration = .seconds(25)) async -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        var skill: SkillContext?
        switch try? workflows.resolve(trimmed) {
        case .runWorkflow(let workflow)?:
            return await runWorkflowAndWait(workflow.id, limit: limit)
        case .runSkill(_, let context)?:
            skill = context
        default:
            break
        }
        guard let task = startRun(trimmed, skill: skill) else {
            return setupError ?? TaskCenter.Refusal.busy.description
        }
        guard let result = await value(of: task, within: limit) else {
            return "Jarvis is still working on it."
        }
        return result.outcome.summary
    }

    /// Starts a task through `TaskCenter`; the run view follows its events.
    @discardableResult
    private func startRun(_ text: String, skill: SkillContext?) -> Task<TaskRunOutcome, Never>? {
        setupError = nil
        switch center.start(text, skill: skill, origin: .owner) {
        case .success(let task):
            requestNotificationPermission()
            return task
        case .failure(let refusal):
            if refusal != .emptyTask { setupError = refusal.description }
            return nil
        }
    }

    /// Stops what runs: the workflow (with its session) or the task.
    func stop() {
        if workflowRunner.isRunning {
            workflowRunner.cancel()
        } else {
            center.cancelSession()
        }
    }

    /// Leaves the finished run view. The Live view's replay and its frames go with it.
    func dismissRun() {
        guard center.session == nil else { return }
        showingRun = false
        live = LiveRun()
        resetViewer()
        coordinator.viewerFrames.removeAll()
    }

    /// The Live view's real-screen frames (memory only).
    var viewerFrames: ViewerFrameStore { coordinator.viewerFrames }

    /// A fresh Live view (a run starts, or the run view goes).
    private func resetViewer(framesEnabled: Bool = false) {
        viewer = LiveViewModel(framesEnabled: framesEnabled)
        viewerTimeline = viewer.timeline
    }

    /// Feeds one event to the Live view (one FFI call; while remote control is on the same
    /// call streams it to paired devices that may watch the screen layout); republishes
    /// only on a change.
    private func feedViewer(_ event: RunEvent) {
        let changed = remote.isOn ? remote.feedLiveView(viewer, event) : viewer.apply(event)
        if changed { viewerTimeline = viewer.timeline }
    }

    // MARK: - Approval preview

    /// The approval sheet's screenshot (one, memory only, held only while asked).
    var approvalFrames: ApprovalFrameStore { coordinator.approvalFrames }

    /// The approval sheet's picture of `prompt`: the core's picture of the pending
    /// confirmation (from the Live view's viewer) bound to that request and the open
    /// question, with the screenshot only when a frame of exactly the evaluated observation
    /// is held, its aspect matches the screen's, and the app is active. `nil` for any other
    /// prompt.
    func approvalPreview(for prompt: HumanPrompt) -> ApprovalPreview? {
        ApprovalPreview.make(prompt: prompt, target: viewer.approvalTarget, frames: coordinator.approvalFrames,
                             appActive: appIsActive)
    }

    /// The wireframe of the evaluated screen (the "Layout from the app's data" / "Layout
    /// from the runner's text" backdrop).
    func approvalScreen(_ preview: ApprovalPreview) -> CoreViewerScreen? {
        viewer.screen(id: preview.screenId)
    }

    /// "Show the real screen when Jarvis asks". Turning it off drops a held frame at once.
    func setApprovalScreenshots(_ on: Bool) {
        settings.approvalScreenshots = on
        if !on { coordinator.approvalFrames.isEnabled = false }
    }

    /// "Show real screen in Live view". Turning it off drops the frames at once, also those
    /// of a run that is going.
    func setLiveViewScreenshots(_ on: Bool) {
        settings.liveViewScreenshots = on
        if !on { coordinator.viewerFrames.isEnabled = false }
    }

    // MARK: - What runs (every task, whoever asked)

    private func taskEvent(_ event: TaskCenter.Event) {
        switch event {
        case .sessionStarted(let session):
            // A run (from Siri, a schedule or a paired device) brings up the run view, which
            // Away mode would cover: leave Away mode so the screen is put back.
            if awayPhase != .off { exitAway() }
            lastTask = session.text
            lastSkill = session.skill
            live = LiveRun(task: session.text, maxSteps: Int(settings.clampedMaxSteps), skillName: session.skill?.name)
            resetViewer(framesEnabled: settings.liveViewScreenshots)
            if session.origin.workflow == nil { showingRun = true }
        case .session(_, let runEvent):
            // A lock abandoned the pending question or confirmation: the broker withdrew that
            // prompt by its id when the driver gave up the wait (never a newer one).
            live.apply(runEvent)
            feedViewer(runEvent)
        case .sessionFinished(let session, let outcome):
            switch outcome {
            case .notStarted(let why):
                setupError = why
                let notStarted = RunEvent.update(.finished(RunResult(
                    outcome: Outcome(status: .failed, summary: why, steps: 0, durationMs: 0),
                    endReason: .sessionFinished, memory: .unavailable(why), effectCount: 0, modelCalls: 0
                )))
                live.apply(notStarted)
                feedViewer(notStarted)
            case .finished:
                if let skill = session.skill { try? skills.recordRun(id: skill.id) }
            }
            Task { await refresh() }
        case .workflowStarted:
            if awayPhase != .off { exitAway() }
            live = LiveRun()
            resetViewer()
            showingWorkflowRun = true
        case .workflowFinished:
            refreshWorkflows()
            Task { await refresh() }
        }
        isRunning = center.isBusy
    }

    // MARK: - Owner answers

    func approve(_ prompt: HumanPrompt) { broker.approve(prompt.id) }
    func decline(_ prompt: HumanPrompt) { broker.decline(prompt.id) }
    func reply(_ prompt: HumanPrompt, _ text: String) { broker.reply(prompt.id, text) }
    func answer(_ prompt: HumanPrompt, yes: Bool) { broker.answerYesNo(prompt.id, yes: yes) }

    /// The sheet was swiped away: a confirmation is declined; a question stays unanswered
    /// (the driver stops the run rather than invent an answer); a yes/no is a no.
    func dismissPrompt(_ prompt: HumanPrompt) {
        switch prompt {
        case .confirm: broker.decline(prompt.id)
        case .question: broker.withdraw(prompt.id)
        case .yesNo: broker.answerYesNo(prompt.id, yes: false)
        }
    }

    // MARK: - Skills

    func refreshSkills() {
        skillList = skills.skills
        skillIssues = skills.loadIssues
        remote.refreshCatalog()
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

    /// Makes the learned flow of `record`'s task a Learned skill named `name`.
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
        for record in history where record.status == .success && record.isWorkflow != true && result[record.task] == nil {
            if let flowID = SkillLibrary.learnedFlowID(forTask: record.task, memoryJSON: memoryJSON, flowIDs: open) {
                result[record.task] = flowID
            }
        }
        promotableFlows = result
    }

    // MARK: - Workflows

    func refreshWorkflows() {
        workflowList = workflows.workflows
        workflowIssues = workflows.loadIssues
        remote.refreshCatalog()
    }

    func saveWorkflow(_ draft: WorkflowDraft) -> Result<Workflow, WorkflowLibraryError> {
        defer { refreshWorkflows() }
        do {
            let workflow = try workflows.save(draft)
            if workflow.hasNotifyStep { requestNotificationPermission() }
            return .success(workflow)
        } catch let error as WorkflowLibraryError {
            return .failure(error)
        } catch {
            return .failure(.core(String(describing: error)))
        }
    }

    func setWorkflowEnabled(_ enabled: Bool, id: String) -> String? {
        defer { refreshWorkflows() }
        do {
            try workflows.setEnabled(enabled, id: id)
            return nil
        } catch {
            return String(describing: error)
        }
    }

    func deleteWorkflow(id: String) -> String? {
        defer { refreshWorkflows() }
        do {
            _ = try workflows.remove(id: id)
            return nil
        } catch {
            return String(describing: error)
        }
    }

    func workflowWarnings(_ draft: WorkflowDraft) -> [String] { workflows.warnings(for: draft) }
    func workflowWarnings(_ workflow: Workflow) -> [String] { workflows.warnings(for: workflow) }

    /// "Run now", a phrase on Ask or a schedule: starts workflow `id`. Returns why it did
    /// not start.
    @discardableResult
    func startWorkflow(_ id: String) -> String? {
        setupError = nil
        switch workflowRunner.start(id: id) {
        case .success:
            return nil
        case .failure(let refusal):
            setupError = refusal.description
            return refusal.description
        }
    }

    /// Runs the workflow a Shortcuts automation names (external key, id or name) and returns
    /// its outcome in one line. Waits at most `limit`: a longer workflow keeps running and
    /// the answer says so.
    func runExternal(_ key: String, limit: Duration = .seconds(25)) async -> String {
        // The first unlock may have happened since the files were last read.
        reloadSettings()
        if let why = WorkflowLibrary.externalUnavailable(settingsRefusal: settingsGate.refusal,
                                                         workflowsBlocked: workflows.isWriteBlocked) {
            return why
        }
        guard let workflow = workflows.find(key) else {
            return "There is no workflow \u{201C}\(key)\u{201D} (or it is off)."
        }
        return await runWorkflowAndWait(workflow.id, limit: limit)
    }

    private func runWorkflowAndWait(_ id: String, limit: Duration = .seconds(25)) async -> String {
        switch workflowRunner.start(id: id) {
        case .failure(let refusal):
            return refusal.description
        case .success(let task):
            let name = workflows.workflow(id: id)?.name ?? id
            // Returns at the limit without cancelling the workflow (a task group would wait
            // for it to end, and iOS would end the intent first).
            guard let outcome = await value(of: task, within: limit) else { return "\(name) is still running in Jarvis." }
            return "\(name): \(outcome.summary)"
        }
    }

    /// Fires due schedules while Jarvis runs (every 20 s, and at once when it comes back).
    func startSchedulerLoop() {
        guard schedulerLoop == nil else { return }
        schedulerLoop = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                self?.checkSchedules()
                try? await Task.sleep(for: .seconds(20))
            }
        }
    }

    func checkSchedules() {
        scheduler.tick(at: LocalTime(date: Date()), runner: workflowRunner)
    }

    /// Minutes until `workflow` next runs on its schedule.
    func minutesUntilNextRun(_ workflow: Workflow, now: Date = Date()) -> UInt32? {
        WorkflowLibrary.minutesUntilNextRun(of: workflow, at: LocalTime(date: now))
    }

    func dismissWorkflowRun() {
        guard !workflowRunner.isRunning else { return }
        showingWorkflowRun = false
        live = LiveRun()
    }

    // MARK: - Remote control

    private func mirrorRemote() {
        remoteStatus = remote.status
        remoteDevices = remote.devices
        remotePending = remote.pendingDevices
        remoteWrongCodes = remote.pairingFailedAttempts
        remotePairing = remote.pairing
        remoteError = remote.lastError
        remoteAudit = remote.audit.entries
        remoteConnected = Set(remote.connected.values)
        remoteWatching = remote.watching
        remoteAddresses = remote.connectionDetails
    }

    func setRemoteEnabled(_ on: Bool) {
        settings.remoteControlEnabled = on
        remote.setEnabled(on)
        mirrorRemote()
        // A paired device's task can wait for an unlock while the phone is in a pocket.
        if on { requestNotificationPermission() }
    }

    /// Opens a pairing offer; returns why it could not.
    func openPairing() -> String? {
        defer { mirrorRemote() }
        do {
            try remote.openPairing()
            return nil
        } catch {
            return String(describing: error)
        }
    }

    func cancelPairing() {
        remote.cancelPairing()
        mirrorRemote()
    }

    /// The offer's time left in whole seconds (0 when expired). Pure: the service's
    /// one-second tick drops an expired offer, and the sheet then shows that it ended.
    func pairingSecondsLeft() -> Int {
        guard let offer = remotePairing else { return 0 }
        let now = remote.nowMs()
        return offer.expiresAtMs > now ? Int((offer.expiresAtMs - now + 999) / 1_000) : 0
    }

    var pairingLockedForSeconds: Int {
        let until = remote.pairingLockedUntilMs
        let now = remote.nowMs()
        return until > now ? Int((until - now + 999) / 1_000) : 0
    }

    func renameDevice(_ id: String, to name: String) -> String? {
        remoteAction { try remote.rename(deviceId: id, to: name) }
    }

    func setPermissions(_ id: String, _ permissions: RemotePermissions) -> String? {
        remoteAction { try remote.setPermissions(deviceId: id, permissions) }
    }

    /// "Watch the screen layout" for a paired device (stored with the device).
    func setWatchScreen(_ id: String, _ on: Bool) -> String? {
        remoteAction { try remote.setWatchScreen(deviceId: id, on) }
    }

    func removeDevice(_ id: String) -> String? {
        remoteAction { try remote.revoke(deviceId: id) }
    }

    /// The owner confirms a device waiting for confirmation (its check string matched the
    /// one on the device they paired).
    func confirmDevice(_ id: String) -> String? {
        remoteAction { try remote.confirm(deviceId: id) }
    }

    private func remoteAction(_ action: () throws -> Void) -> String? {
        defer { mirrorRemote() }
        do {
            try action()
            return nil
        } catch {
            return String(describing: error)
        }
    }

    /// Categories no device can ever approve (from the core).
    var neverRemoteCategories: [RiskCategory] { (try? neverRemoteApprove()) ?? [] }

    // MARK: - Controlling another phone

    private func mirrorController() {
        let state = controller.state
        if state != controllerState { controllerState = state }
        let timeline = controller.live.model.timeline
        if timeline != controllerTimeline { controllerTimeline = timeline }
    }

    /// The controller screen appeared: connect with the stored pairing, if any.
    func controllerActivate() {
        controller.activate()
        mirrorController()
    }

    /// The name this device gives a phone it pairs with, unless the owner types another
    /// (`UIDevice.current.name` reads "iPhone" without the device-name entitlement).
    var controllerDefaultDeviceName: String { controller.defaultDeviceName }

    /// Pairs with the phone at the typed address with the typed code under `name`; returns
    /// why not.
    func controllerPair(code: String, host: String, port: String, name: String? = nil) async -> String? {
        let address: RemotePhoneAddress
        switch RemotePhoneAddress.parse(host: host, port: port) {
        case .success(let parsed): address = parsed
        case .failure(let problem): return problem.description
        }
        return await controllerPair(code: Redacted(code), address: address, name: name)
    }

    /// Pairs with a phone found on this Wi-Fi (or the in-process one).
    func controllerPair(code: Redacted<String>, address: RemotePhoneAddress, name: String? = nil) async -> String? {
        defer { mirrorController() }
        do {
            try await controller.pair(code: code, address: address, name: name)
            return nil
        } catch {
            return String(describing: error)
        }
    }

    /// QA (playground only): opens a pairing offer on this phone and pairs the controller
    /// with it over the in-memory transport. The code is handed over in-process: it stays
    /// out of the accessibility tree (the pairing sheet labels it "Pairing code" on purpose).
    /// The new device then waits for the owner's Confirm on this phone's Remote control
    /// screen, like any other (the UI test taps that row).
    func controllerPairWithThisPhoneForQA(name: String? = nil) async -> String? {
        guard launch.playground else { return "Only in playground mode." }
        if !settings.remoteControlEnabled { setRemoteEnabled(true) }
        let offer: RemotePairingOffer
        do {
            offer = try remote.openPairing()
        } catch {
            mirrorRemote()
            return String(describing: error)
        }
        mirrorRemote()
        let failure = await controllerPair(code: offer.code, address: .inProcess, name: name)
        mirrorRemote()
        return failure
    }

    func controllerStart(_ text: String) -> String? { controllerAction { try controller.startTask(text) } }
    func controllerStop() -> String? { controllerAction { try controller.stop() } }
    func controllerAnswer(approved: Bool) -> String? { controllerAction { try controller.answer(approved: approved) } }
    func controllerWatch(_ on: Bool) -> String? {
        controllerAction {
            if on {
                try controller.startWatching()
            } else {
                try controller.stopWatching()
            }
        }
    }

    func controllerReconnect() {
        controller.reconnect()
        mirrorController()
    }

    func controllerDisconnect() {
        controller.disconnect()
        mirrorController()
    }

    func controllerForget() {
        controller.forget()
        mirrorController()
    }

    /// The owner says the phone showed this device's check string when they confirmed it.
    func controllerCheckMatched() {
        controller.confirmCheckMatched()
        mirrorController()
    }

    private func controllerAction(_ action: () throws -> Void) -> String? {
        defer { mirrorController() }
        do {
            try action()
            return nil
        } catch {
            return String(describing: error)
        }
    }

    /// QA: pairs "Test Mac" in-process over the in-memory transport and confirms it, as the
    /// owner would after comparing the check strings (playground only; the confirm step's
    /// own UI test confirms through the Remote control screen instead).
    private func seedRemoteDevice() {
        Task { @MainActor [weak self] in
            guard let self, self.remote.isOn else { return }
            guard (self.remote.devices).isEmpty else { return }
            do {
                let offer = try self.remote.openPairing()
                let seeded = try await InProcessRemote.pair(code: offer.code, name: "Test Mac", over: self.memoryTransport,
                                                           random: SecureRandomSource())
                self.seededRemote = seeded
                // The hub keeps the device (pending) once its first frame arrives, which can
                // be just after the client says it is connected.
                var confirmed = false
                for _ in 0..<500 where !confirmed {
                    if let id = seeded.deviceId, self.remote.pendingDevices.contains(where: { $0.device.id == id }) {
                        try self.remote.confirm(deviceId: id)
                        confirmed = true
                    } else {
                        try await Task.sleep(for: .milliseconds(10))
                    }
                }
                if !confirmed { self.remoteError = "The QA device paired but could not be confirmed." }
            } catch {
                self.remoteError = "The QA device could not pair: \(error)"
            }
            self.mirrorRemote()
        }
    }

    // MARK: - Apps

    var appAccess: [AppAccess] {
        AppAccessCatalog.make(settings: settings, knownApps: coordinator.knownApps, tools: toolSpecs,
                              coreDeniedApps: coreDeniedApps)
    }

    var skillAppChoices: [AppAccess] { appAccess.filter(\.screen) }

    func name(ofApp bundleId: String) -> String {
        appAccess.first { $0.bundleId == bundleId }?.name ?? bundleId
    }

    // MARK: - Away mode

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

    /// Scene phase changes (RootView): the screen is put back whenever Jarvis is not active;
    /// schedules are checked when it comes back.
    func sceneBecameActive(_ active: Bool) {
        // The approval sheet's screenshot is drawn only while Jarvis is in front
        // (`approvalPreview(for:)` reads this). It is captured and held while Jarvis is in
        // the background: when Jarvis asks, the target app is in front, and the owner is on
        // the way to the sheet. It goes when the question is resolved or the phone locks.
        appIsActive = active
        if active {
            reloadSettings()
            away.appReturnedToForeground()
            checkSchedules()
            // An NWListener often fails while the app is suspended: listen again.
            if remote.ensureListening() { mirrorRemote() }
            refreshNotificationAccess()
        } else {
            away.appLeftForeground()
        }
        awayPhase = away.phase
    }

    private func appWillTerminate() {
        away.appWillTerminate()
        awayPhase = away.phase
    }

    // MARK: - Notifications

    /// Asks for notification permission while Jarvis is in front, where the system can show
    /// the alert (once per launch; the system itself asks only once). Called when the owner
    /// turns on remote control, saves a workflow with a Notify step or starts a task. Never
    /// in playground mode (UI tests must not meet a system alert).
    private func requestNotificationPermission() {
        guard !launch.playground, !askedForNotifications,
              UIApplication.shared.applicationState == .active else { return }
        askedForNotifications = true
        Task { @MainActor [weak self] in
            let access = await NotificationPermission.request()
            self?.notificationAccess = access
        }
    }

    /// Reads the permission again (the owner may have changed it in Settings).
    private func refreshNotificationAccess() {
        guard !launch.playground else { return }
        Task { @MainActor [weak self] in
            let access = await NotificationPermission.current()
            self?.notificationAccess = access
        }
    }

    // MARK: - The owner's rules

    /// Reads the rules again while they could not be read (the app became active, or
    /// protected data became available after the first unlock).
    func reloadSettings() {
        if settingsGate.refresh() {
            let loaded = settingsGate.settings
            if settings != loaded { settings = loaded }
            // Remote control follows the rules file both ways.
            if settings.remoteControlEnabled != remote.isOn {
                remote.setEnabled(settings.remoteControlEnabled)
                mirrorRemote()
            }
            away.applySaved(acknowledged: settings.awayAcknowledged, proximity: settings.awayProximity)
        }
        if [skills.reloadIfBlocked(), workflows.reloadIfBlocked()].contains(true) { refreshSkills(); refreshWorkflows() }
        mirrorSettingsProblem()
    }

    /// "I have reviewed my rules" after a damaged rules file was set aside.
    func acknowledgeRulesReview() {
        settingsGate.acknowledgeReview()
        mirrorSettingsProblem()
    }

    private func mirrorSettingsProblem() {
        settingsProblem = settingsGate.refusal
        if case .damaged? = settingsGate.problem {
            settingsNeedReview = true
        } else {
            settingsNeedReview = false
        }
    }

    // MARK: - Data

    func refresh() async {
        history = historyStore.load()
        memory = await coordinator.memorySnapshot()
        toolSpecs = await coordinator.toolSpecs()
        refreshSkills()
        refreshWorkflows()
        refreshPromotable(memoryJSON: await coordinator.memoryJSON())
        mirrorRemote()
    }

    func checkConnections() async {
        if launch.playground {
            runnerStatus = .reachable("demo phone (playground mode)")
            modelStatus = .reachable("scripted demo model (playground mode)")
            return
        }
        let probe = ConnectionProbe()
        if settings.runnerKind == .jarvis {
            async let model = probe.model(settings.modelBaseURL)
            do {
                // One cheap authenticated request: the runner proved the key and answered.
                _ = try await ownRunner.frontBundleId()
                runnerStatus = .reachable("Jarvis runner answered and proved its key")
            } catch {
                runnerStatus = .unreachable(String(describing: error))
            }
            modelStatus = await model
            return
        }
        async let runner = probe.runner(settings.runnerBaseURL)
        async let model = probe.model(settings.modelBaseURL)
        runnerStatus = await runner
        modelStatus = await model
    }

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
