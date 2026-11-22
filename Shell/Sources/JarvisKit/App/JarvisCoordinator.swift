import Foundation
import JarvisCore
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Where the app keeps its files.
public struct JarvisPaths: Sendable, Equatable {
    public var directory: URL

    public init(directory: URL) {
        self.directory = directory
    }

    public var settings: URL { directory.appendingPathComponent("settings.json") }
    public var history: URL { directory.appendingPathComponent("history.json") }
    /// The core's memory for real runs.
    public var memory: URL { directory.appendingPathComponent("memory.json") }
    /// A separate memory for demo runs, so demo flows never mix with real ones.
    public var demoMemory: URL { directory.appendingPathComponent("demo-memory.json") }
    /// The owner's skills (`SkillLibrary`).
    public var skills: URL { directory.appendingPathComponent("skills.json") }
    /// The owner's workflows (`WorkflowLibrary`).
    public var workflows: URL { directory.appendingPathComponent("workflows.json") }
    /// Which scheduled minutes already fired (`WorkflowScheduler`).
    public var workflowSchedule: URL { directory.appendingPathComponent("workflow-schedule.json") }
    /// Remote control's audit log (no secrets: the core keeps them out of audit entries).
    public var remoteAudit: URL { directory.appendingPathComponent("remote-audit.json") }
}

/// Which IO a run uses.
public enum BackendSelection: Sendable, Equatable {
    /// The LoopCtrl runner and the model backend from the settings.
    case live
    /// `DemoPhone` and a `ScriptedModel`: no runner, no model server (app demo mode and
    /// the app's own UI tests).
    case demo
}

/// Builds the model backend the settings select. The one place a backend is chosen: an
/// on-device runtime becomes a `ModelBackendKind` case and a branch here.
public enum ModelBackendFactory {
    public enum FactoryError: Error, Equatable, CustomStringConvertible {
        case invalidBaseURL(String)

        public var description: String {
            switch self {
            case .invalidBaseURL(let s): "the model base URL \"\(s)\" is not a valid http(s) URL"
            }
        }
    }

    public static func make(settings: JarvisSettings, screenshots: ScreenshotStore, transport: (any HTTPTransport)? = nil) throws -> any ModelProvider {
        switch settings.modelBackend {
        case .openAICompatible:
            let url = try validURL(settings.modelBaseURL)
            let configuration = OpenAICompatibleModel.Configuration(
                baseURL: url,
                model: settings.modelName,
                capabilities: ModelCapabilities(
                    contextTokens: UInt32(min(max(settings.modelContextTokens, 1_024), 1_000_000)),
                    supportsVision: false,
                    constrainedDecoding: .jsonSchema
                )
            )
            return OpenAICompatibleModel(configuration: configuration, screenshots: screenshots, transport: transport)
        }
    }

    static func validURL(_ text: String) throws -> URL {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: trimmed), let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https", url.host != nil else {
            throw FactoryError.invalidBaseURL(text)
        }
        return url
    }
}

/// Apps the agent can open by name.
public enum KnownApps {
    public static let common: [AppInfo] = [
        AppInfo(bundleId: "com.apple.Preferences", name: "Settings"),
        AppInfo(bundleId: "com.apple.mobilesafari", name: "Safari"),
        AppInfo(bundleId: "com.apple.MobileSMS", name: "Messages"),
        AppInfo(bundleId: "com.apple.mobilenotes", name: "Notes"),
        AppInfo(bundleId: "com.apple.mobilecal", name: "Calendar"),
        AppInfo(bundleId: "com.apple.reminders", name: "Reminders"),
        AppInfo(bundleId: "com.apple.mobileslideshow", name: "Photos"),
        AppInfo(bundleId: "com.apple.mobilemail", name: "Mail"),
        AppInfo(bundleId: "com.apple.Maps", name: "Maps"),
        AppInfo(bundleId: "com.apple.mobiletimer", name: "Clock"),
        AppInfo(bundleId: "com.apple.DocumentsApp", name: "Files"),
        AppInfo(bundleId: "com.apple.MobileAddressBook", name: "Contacts"),
        AppInfo(bundleId: "com.apple.mobilephone", name: "Phone"),
        AppInfo(bundleId: "com.apple.Passbook", name: "Wallet"),
        AppInfo(bundleId: "com.apple.Passwords", name: "Passwords"),
    ]
}

/// A `ModelProvider` wrapper that reports each prompt (user part) and reply.
public final class TappedModel: ModelProvider {
    private let inner: any ModelProvider
    private let onPrompt: @Sendable (ModelRequest) -> Void
    private let onReply: @Sendable (String) -> Void

    public init(_ inner: any ModelProvider, onPrompt: @escaping @Sendable (ModelRequest) -> Void, onReply: @escaping @Sendable (String) -> Void) {
        self.inner = inner
        self.onPrompt = onPrompt
        self.onReply = onReply
    }

    public var capabilities: ModelCapabilities { inner.capabilities }

    public func generate(_ request: ModelRequest) async throws -> String {
        onPrompt(request)
        let reply = try await inner.generate(request)
        onReply(reply)
        return reply
    }
}

/// How a task request ended.
public enum TaskRunOutcome: Sendable, Equatable {
    /// The session ran (in any way, including failure or cancellation).
    case finished(TaskSpec, RunResult)
    /// The run could not be set up (bad settings, core rejected the config).
    case notStarted(String)
}

/// Builds a `SessionDriver` from the settings and runs one task: loads the memory (and
/// quarantines memory the core rejects), picks the runner and model for the selection,
/// streams `RunEvent`s, and records the result in the task history. Views never touch the
/// driver directly; to stop a run, cancel the task that awaits `run`.
public final class JarvisCoordinator: Sendable {
    public let paths: JarvisPaths
    public let selection: BackendSelection
    /// The app's own bundle id (never needs first-use approval).
    public let agentBundleId: String
    private let tools: ToolRegistry
    private let configuration: DriverConfiguration
    private let demoDelay: Duration
    private let demoLockedPolls: Int
    private let maxLockedWaitOverrideMs: UInt64?

    public init(
        paths: JarvisPaths,
        selection: BackendSelection,
        agentBundleId: String = "dev.jarvis.app",
        tools: ToolRegistry? = nil,
        configuration: DriverConfiguration = DriverConfiguration(),
        demoDelay: Duration = .milliseconds(350),
        demoLockedPolls: Int = 0,
        maxLockedWaitOverrideMs: UInt64? = nil
    ) {
        self.paths = paths
        self.selection = selection
        self.agentBundleId = agentBundleId
        self.configuration = configuration
        self.demoDelay = demoDelay
        self.demoLockedPolls = demoLockedPolls
        self.maxLockedWaitOverrideMs = maxLockedWaitOverrideMs
        if let tools {
            self.tools = tools
        } else {
            let registry = ToolRegistry()
            SampleTools.registerClockNow(in: registry)
            self.tools = registry
        }
    }

    public var memoryURL: URL { selection == .demo ? paths.demoMemory : paths.memory }

    /// The apps the agent knows by name for this selection.
    public var knownApps: [AppInfo] { selection == .demo ? DemoPhone.knownApps : KnownApps.common }

    /// The tools runs offer the core (the Apps screen derives "app actions" and "direct"
    /// from them).
    public func toolSpecs() async -> [ToolSpec] { await tools.toolSpecs() }

    /// The memory JSON as saved (for promoting a learned flow to a skill).
    public func memoryJSON() async -> String {
        (try? await FileMemoryStore(fileURL: memoryURL).load()) ?? ""
    }

    /// The memory as the Activity tab reads it.
    public func memorySnapshot() async -> MemorySnapshot {
        let json = (try? await FileMemoryStore(fileURL: memoryURL).load()) ?? ""
        return MemorySnapshot(json: json)
    }

    /// The demo script for a task text (Delete-account task or the Wi-Fi task).
    public static func demoScript(for text: String) -> [ScriptStep] {
        text.lowercased().contains("delete") ? DemoPhone.deleteAccountScript : DemoPhone.wifiOffScript
    }

    /// Runs `text` to the end, under `skill` when given (from `WorkflowLibrary.resolve`,
    /// `SkillLibrary.context(…)`; the core puts its guidance in the prompt and, when
    /// confined, refuses actions outside its apps). `origin` only tags the history record.
    /// `onEvent` is called from the driver's context.
    ///
    /// Apps call this through `TaskCenter.start`, the one entry every request takes (Ask,
    /// Siri, Run now, a workflow step, a paired device), so all of them get the same policy,
    /// the same confirmation gate and the same lock handling.
    public func run(
        task text: String,
        settings: JarvisSettings,
        skill: SkillContext? = nil,
        origin: TaskOrigin = .owner,
        human: any HumanGate,
        attention: any OwnerAttention = NoOwnerAttention(),
        onEvent: @escaping @Sendable (RunEvent) -> Void
    ) async -> TaskRunOutcome {
        let task = TaskSpec(id: UUID().uuidString, text: text)
        let screenshots = ScreenshotStore()
        let runner: any PhoneRunner
        let baseModel: any ModelProvider
        let knownApps: [AppInfo]
        switch selection {
        case .demo:
            runner = DemoPhone(delay: demoDelay, lockedPolls: demoLockedPolls)
            baseModel = ScriptedModel(steps: Self.demoScript(for: text))
            knownApps = self.knownApps
        case .live:
            do {
                baseModel = try ModelBackendFactory.make(settings: settings, screenshots: screenshots)
                let runnerURL = try ModelBackendFactory.validURL(settings.runnerBaseURL)
                // LoopCtrl can say whether the screen is locked (`device_info`): check it after
                // every command, so a tap that landed on the lock screen is not trusted.
                runner = LockCheckingRunner(LoopCtrlRunner(configuration: LoopCtrlRunner.Configuration(baseURL: runnerURL)))
            } catch {
                return .notStarted(String(describing: error))
            }
            knownApps = self.knownApps
        }
        let model = TappedModel(
            baseModel,
            onPrompt: { onEvent(.prompt($0.user)) },
            onReply: { onEvent(.reply($0)) }
        )

        let config: SessionConfig
        do {
            config = try SessionConfigBuilder.make(
                capabilities: model.capabilities,
                tools: await tools.toolSpecs(),
                knownApps: knownApps,
                agentBundleId: agentBundleId,
                skill: skill,
                adjustPolicy: { [maxLockedWaitOverrideMs] policy in
                    settings.apply(to: &policy)
                    // The app's UI tests: a lock that never clears must end the run in seconds.
                    if let maxLockedWaitOverrideMs { policy.maxLockedWaitMs = maxLockedWaitOverrideMs }
                }
            )
        } catch {
            return .notStarted("the core rejected the configuration: \(error)")
        }

        let memory = FileMemoryStore(fileURL: memoryURL)
        let memoryJSON = (try? await memory.load()) ?? ""
        let deps = DriverDependencies(
            runner: runner, model: model, tools: tools, human: human, memory: memory, screenshots: screenshots,
            attention: attention
        )
        let driver: SessionDriver
        do {
            driver = try SessionDriver(config: config, task: task, memoryJSON: memoryJSON, dependencies: deps, configuration: configuration)
        } catch {
            // The core rejected the saved memory: set it aside and start fresh.
            _ = try? await memory.quarantine()
            do {
                driver = try SessionDriver(config: config, task: task, memoryJSON: "", dependencies: deps, configuration: configuration)
            } catch {
                return .notStarted("the core could not start a session: \(error)")
            }
        }

        let forward = Task {
            var apps: [String] = []
            for await update in driver.updates {
                if case .progress(let p) = update, let app = p.app, !apps.contains(app) { apps.append(app) }
                onEvent(.update(update))
            }
            return apps
        }
        let result = await withTaskCancellationHandler {
            await driver.run()
        } onCancel: {
            Task { await driver.cancel() }
        }
        let apps = await forward.value
        _ = try? TaskHistoryStore(fileURL: paths.history).append(TaskRecord(task: task, result: result, apps: apps, skill: skill, origin: origin))
        return .finished(task, result)
    }
}

/// Whether a runner or model endpoint answers.
public enum ConnectionStatus: Sendable, Equatable {
    case unknown
    case reachable(String)
    case unreachable(String)

    public var isReachable: Bool {
        if case .reachable = self { return true }
        return false
    }
}

/// Probes endpoints with one GET each: any HTTP answer means the server is up.
public struct ConnectionProbe: Sendable {
    private let transport: any HTTPTransport

    public init(transport: (any HTTPTransport)? = nil, timeout: TimeInterval = 3) {
        self.transport = transport ?? URLSessionTransport(timeout: timeout)
    }

    /// LoopCtrl answers `/whoami` without pairing [readme]; any HTTP status counts.
    public func runner(_ baseURL: String) async -> ConnectionStatus {
        await probe(baseURL, path: "/whoami", what: "runner")
    }

    /// OpenAI-compatible servers list models at `/v1/models`.
    public func model(_ baseURL: String) async -> ConnectionStatus {
        await probe(baseURL, path: "/v1/models", what: "model server")
    }

    func probe(_ base: String, path: String, what: String) async -> ConnectionStatus {
        guard let url = try? ModelBackendFactory.validURL(base) else {
            return .unreachable("\"\(base)\" is not a valid http(s) URL")
        }
        var request = URLRequest(url: url.appendingPathComponent(String(path.dropFirst())))
        request.httpMethod = "GET"
        do {
            let (_, response) = try await transport.send(request)
            return .reachable("\(what) answered HTTP \(response.statusCode)")
        } catch {
            return .unreachable("\(what) at \(url.absoluteString): \(error)")
        }
    }
}
