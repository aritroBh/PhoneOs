import Foundation
import JarvisCore

/// Codable mirror of the core's `RiskCategory` (the generated enum is not Codable).
public enum RiskCategoryOption: String, Codable, CaseIterable, Sendable, Identifiable {
    case sendMessage = "send_message"
    case payment
    case purchase
    case delete
    case publicPost = "public_post"
    case accountChange = "account_change"
    case systemSetting = "system_setting"
    case credential
    case unverified
    case call

    public var id: String { rawValue }

    public var core: RiskCategory {
        switch self {
        case .sendMessage: .sendMessage
        case .payment: .payment
        case .purchase: .purchase
        case .delete: .delete
        case .publicPost: .publicPost
        case .accountChange: .accountChange
        case .systemSetting: .systemSetting
        case .credential: .credential
        case .unverified: .unverified
        case .call: .call
        }
    }

    /// The option for a core category (`nil` for `none` and `firstUse`, which the settings
    /// do not expose as categories).
    public init?(_ category: RiskCategory) {
        guard let match = Self.allCases.first(where: { $0.core == category }) else { return nil }
        self = match
    }

    /// Owner-facing name.
    public var title: String {
        switch self {
        case .sendMessage: "Sending a message"
        case .payment: "Paying"
        case .purchase: "Buying"
        case .delete: "Deleting"
        case .publicPost: "Posting publicly"
        case .accountChange: "Changing an account"
        case .systemSetting: "Changing a system setting"
        case .credential: "Entering credentials"
        case .unverified: "Unreadable controls"
        case .call: "Placing a call"
        }
    }
}

/// Short owner-facing name of any core category (the chip on confirmation sheets).
public func riskCategoryTitle(_ category: RiskCategory) -> String {
    switch category {
    case .none: "Action"
    case .sendMessage: "Send message"
    case .payment: "Payment"
    case .purchase: "Purchase"
    case .delete: "Delete"
    case .publicPost: "Public post"
    case .accountChange: "Account change"
    case .systemSetting: "System setting"
    case .credential: "Credentials"
    case .unverified: "Unreadable control"
    case .call: "Phone call"
    case .firstUse: "First time in this app"
    }
}

/// Codable mirror of `TrustLevel`.
public enum TrustLevelOption: String, Codable, CaseIterable, Sendable, Identifiable {
    case trusted
    case guarded
    case stepByStep = "step_by_step"
    case denied

    public var id: String { rawValue }

    public var core: TrustLevel {
        switch self {
        case .trusted: .trusted
        case .guarded: .guarded
        case .stepByStep: .stepByStep
        case .denied: .denied
        }
    }

    public var title: String {
        switch self {
        case .trusted: "Trusted"
        case .guarded: "Guarded"
        case .stepByStep: "Step by step"
        case .denied: "Denied"
        }
    }

    /// One line on what the level means, for the app named `app`.
    public func explanation(app: String) -> String {
        switch self {
        case .trusted: "Jarvis acts in \(app) without asking, except to pay, buy, delete, send, post or call."
        case .guarded: "Jarvis asks before the risky steps in \(app) and does the rest."
        case .stepByStep: "Jarvis asks before every action in \(app), not only the risky ones."
        case .denied: "Jarvis never opens or acts in \(app), whatever the task says."
        }
    }
}

/// Codable mirror of `PolicyMode`.
public enum PolicyModeOption: String, Codable, CaseIterable, Sendable, Identifiable {
    case guarded
    case stepByStep = "step_by_step"

    public var id: String { rawValue }

    public var core: PolicyMode {
        switch self {
        case .guarded: .guarded
        case .stepByStep: .stepByStep
        }
    }

    public var title: String {
        switch self {
        case .guarded: "Guarded"
        case .stepByStep: "Step by step"
        }
    }

    public var explanation: String {
        switch self {
        case .guarded: "Asks before the risky steps and does the rest."
        case .stepByStep: "Asks before every action that changes something."
        }
    }
}

/// Which model backend runs the agent. The single selection point for backends: on-device
/// runtimes are added here as cases and built in `ModelBackendFactory`.
public enum ModelBackendKind: String, Codable, CaseIterable, Sendable {
    /// An OpenAI-compatible HTTP server (`OpenAICompatibleModel`).
    case openAICompatible = "openai_compatible"
}

/// One app's trust level, as the owner set it.
public struct AppTrust: Codable, Sendable, Equatable, Identifiable {
    /// Bundle id or prefix (matched at `.` boundaries by the core).
    public var bundleIdPrefix: String
    /// Display name.
    public var name: String
    public var level: TrustLevelOption

    public var id: String { bundleIdPrefix }

    public init(bundleIdPrefix: String, name: String, level: TrustLevelOption) {
        self.bundleIdPrefix = bundleIdPrefix
        self.name = name
        self.level = level
    }
}

/// Everything the owner can set, persisted as JSON. Every field is optional when decoding
/// (missing fields take the defaults), so older files keep loading; a safety field that is
/// present but cannot be decoded makes the file damaged (see `init(from:)`).
public struct JarvisSettings: Codable, Sendable, Equatable {
    public var policyMode: PolicyModeOption
    /// Ask the first time Jarvis acts in an app.
    public var firstUseApproval: Bool
    /// Categories the owner chose not to be asked about (removed from the core's default
    /// confirmation list). Empty by default: the core asks for every risky category.
    public var relaxedCategories: [RiskCategoryOption]
    /// Categories never executed.
    public var denyCategories: [RiskCategoryOption]
    /// Step budget per task.
    public var maxSteps: Int
    /// Time budget per task, in minutes.
    public var maxMinutes: Int
    /// How long a run waits for the owner to unlock the phone before it ends Interrupted,
    /// in minutes (the core's `max_locked_wait_ms`).
    public var maxLockedWaitMinutes: Int
    public var modelBackend: ModelBackendKind
    /// OpenAI-compatible server base URL.
    public var modelBaseURL: String
    /// The `model` field sent to it.
    public var modelName: String
    /// Context window assumed for the model.
    public var modelContextTokens: Int
    /// The LoopCtrl runner.
    public var runnerBaseURL: String
    /// Per-app trust, feeding `Policy.appRules`.
    public var appTrust: [AppTrust]
    /// The owner accepted what Away mode costs (shown once, before first use).
    public var awayAcknowledged: Bool
    /// Away mode blanks the display with the proximity sensor (off by default).
    public var awayProximity: Bool
    /// Remote control's listener runs (off by default; the paired devices live in the
    /// Keychain, not here).
    public var remoteControlEnabled: Bool

    public static let defaultAppTrust: [AppTrust] = [
        AppTrust(bundleIdPrefix: "com.apple.Preferences", name: "Settings", level: .guarded),
        AppTrust(bundleIdPrefix: "com.apple.MobileSMS", name: "Messages", level: .stepByStep),
        AppTrust(bundleIdPrefix: "com.apple.mobilesafari", name: "Safari", level: .guarded),
        AppTrust(bundleIdPrefix: "com.apple.mobilenotes", name: "Notes", level: .guarded),
        AppTrust(bundleIdPrefix: "com.apple.Passbook", name: "Wallet", level: .denied),
    ]

    public init(
        policyMode: PolicyModeOption = .guarded,
        firstUseApproval: Bool = true,
        relaxedCategories: [RiskCategoryOption] = [],
        denyCategories: [RiskCategoryOption] = [],
        maxSteps: Int = 40,
        maxMinutes: Int = 10,
        maxLockedWaitMinutes: Int = 30,
        modelBackend: ModelBackendKind = .openAICompatible,
        modelBaseURL: String = "http://127.0.0.1:8080",
        modelName: String = "local-model",
        modelContextTokens: Int = 8_192,
        runnerBaseURL: String = "http://127.0.0.1:8300",
        appTrust: [AppTrust] = JarvisSettings.defaultAppTrust,
        awayAcknowledged: Bool = false,
        awayProximity: Bool = false,
        remoteControlEnabled: Bool = false
    ) {
        self.policyMode = policyMode
        self.firstUseApproval = firstUseApproval
        self.relaxedCategories = relaxedCategories
        self.denyCategories = denyCategories
        self.maxSteps = maxSteps
        self.maxMinutes = maxMinutes
        self.maxLockedWaitMinutes = maxLockedWaitMinutes
        self.modelBackend = modelBackend
        self.modelBaseURL = modelBaseURL
        self.modelName = modelName
        self.modelContextTokens = modelContextTokens
        self.runnerBaseURL = runnerBaseURL
        self.appTrust = appTrust
        self.awayAcknowledged = awayAcknowledged
        self.awayProximity = awayProximity
        self.remoteControlEnabled = remoteControlEnabled
    }

    enum CodingKeys: String, CodingKey {
        case policyMode = "policy_mode"
        case firstUseApproval = "first_use_approval"
        case relaxedCategories = "relaxed_categories"
        case denyCategories = "deny_categories"
        case maxSteps = "max_steps"
        case maxMinutes = "max_minutes"
        case maxLockedWaitMinutes = "max_locked_wait_minutes"
        case modelBackend = "model_backend"
        case modelBaseURL = "model_base_url"
        case modelName = "model_name"
        case modelContextTokens = "model_context_tokens"
        case runnerBaseURL = "runner_base_url"
        case appTrust = "app_trust"
        case awayAcknowledged = "away_acknowledged"
        case awayProximity = "away_proximity"
        case remoteControlEnabled = "remote_control_enabled"
    }

    /// Decoding has two rules. A **missing** key takes the default, so older files keep
    /// loading. A key that is **present** but cannot be decoded (wrong type, `null`, an enum
    /// value this build does not know, also inside a list) fails the whole decode when the
    /// field is one of the owner's safety rules: the policy mode, first-use approval, the
    /// relaxed and denied categories, the app trust list, remote control, and the step, time
    /// and lock-wait limits. `SettingsStore.read()` then reports the file as damaged and sets
    /// it aside, so no task runs under weaker rules than the owner chose. The other fields
    /// (model and runner addresses, the model's context size, Away mode) take the default
    /// when their value cannot be decoded.
    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = JarvisSettings()
        policyMode = try Self.strict(c, .policyMode, d.policyMode)
        firstUseApproval = try Self.strict(c, .firstUseApproval, d.firstUseApproval)
        relaxedCategories = try Self.strict(c, .relaxedCategories, d.relaxedCategories)
        denyCategories = try Self.strict(c, .denyCategories, d.denyCategories)
        maxSteps = try Self.strict(c, .maxSteps, d.maxSteps)
        maxMinutes = try Self.strict(c, .maxMinutes, d.maxMinutes)
        maxLockedWaitMinutes = try Self.strict(c, .maxLockedWaitMinutes, d.maxLockedWaitMinutes)
        modelBackend = (try? c.decodeIfPresent(ModelBackendKind.self, forKey: .modelBackend)) ?? d.modelBackend
        modelBaseURL = (try? c.decodeIfPresent(String.self, forKey: .modelBaseURL)) ?? d.modelBaseURL
        modelName = (try? c.decodeIfPresent(String.self, forKey: .modelName)) ?? d.modelName
        modelContextTokens = (try? c.decodeIfPresent(Int.self, forKey: .modelContextTokens)) ?? d.modelContextTokens
        runnerBaseURL = (try? c.decodeIfPresent(String.self, forKey: .runnerBaseURL)) ?? d.runnerBaseURL
        appTrust = try Self.strict(c, .appTrust, d.appTrust)
        awayAcknowledged = (try? c.decodeIfPresent(Bool.self, forKey: .awayAcknowledged)) ?? d.awayAcknowledged
        awayProximity = (try? c.decodeIfPresent(Bool.self, forKey: .awayProximity)) ?? d.awayProximity
        remoteControlEnabled = try Self.strict(c, .remoteControlEnabled, d.remoteControlEnabled)
    }

    /// The keys whose value must decode when present (see `init(from:)`).
    static let safetyKeys: [CodingKeys] = [
        .policyMode, .firstUseApproval, .relaxedCategories, .denyCategories, .maxSteps, .maxMinutes,
        .maxLockedWaitMinutes, .appTrust, .remoteControlEnabled,
    ]

    /// A safety field: the default when the key is missing, the decoded value when present,
    /// and an error naming the key when the value is there but cannot be used.
    private static func strict<T: Decodable>(_ c: KeyedDecodingContainer<CodingKeys>, _ key: CodingKeys, _ fallback: T) throws -> T {
        guard c.contains(key) else { return fallback }
        do {
            return try c.decode(T.self, forKey: key)
        } catch {
            throw DecodingError.dataCorrupted(DecodingError.Context(
                codingPath: [key],
                debugDescription: "\"\(key.rawValue)\" holds a value this version of Jarvis cannot use (\(StoredFile.describe(error)))",
                underlyingError: error
            ))
        }
    }

    // MARK: - Validated values

    /// `maxSteps` clamped to 1…500.
    public var clampedMaxSteps: UInt32 { UInt32(min(max(maxSteps, 1), 500)) }
    /// `maxMinutes` clamped to 1…120, in milliseconds.
    public var maxDurationMs: UInt64 { UInt64(min(max(maxMinutes, 1), 120)) * 60_000 }

    /// `maxLockedWaitMinutes` clamped to 1…240, in milliseconds.
    public var maxLockedWaitMs: UInt64 { UInt64(min(max(maxLockedWaitMinutes, 1), 240)) * 60_000 }

    /// Applies the settings on top of the core's default policy. The core's own defaults
    /// stay for everything not exposed here (the denied Passwords and Wallet apps, failure
    /// and parse-retry caps, categories added to the core later).
    public func apply(to policy: inout Policy) {
        policy.mode = policyMode.core
        policy.firstUseApproval = firstUseApproval
        let relaxed = Set(relaxedCategories.map(\.core))
        policy.confirm.removeAll { relaxed.contains($0) }
        var deny = policy.denyCategories
        for category in denyCategories.map(\.core) where !deny.contains(category) {
            deny.append(category)
        }
        policy.denyCategories = deny
        policy.maxSteps = clampedMaxSteps
        policy.maxDurationMs = maxDurationMs
        policy.maxLockedWaitMs = maxLockedWaitMs
        policy.appRules = appTrust
            .filter { !$0.bundleIdPrefix.trimmingCharacters(in: .whitespaces).isEmpty }
            .map { AppRule(bundleIdPrefix: $0.bundleIdPrefix.trimmingCharacters(in: .whitespaces), level: $0.level.core) }
    }

    /// Whether the core still asks for `category` under these settings.
    public func asksBefore(_ category: RiskCategoryOption) -> Bool {
        !relaxedCategories.contains(category)
    }

    /// Sets whether the core asks for `category`.
    public mutating func setAsksBefore(_ category: RiskCategoryOption, _ asks: Bool) {
        relaxedCategories.removeAll { $0 == category }
        if !asks { relaxedCategories.append(category) }
    }

    /// The trust level set for `bundleId` (longest matching prefix), `nil` when none.
    public func trust(for bundleId: String) -> TrustLevelOption? {
        appTrust
            .filter { bundleId == $0.bundleIdPrefix || bundleId.hasPrefix($0.bundleIdPrefix + ".") }
            .max { $0.bundleIdPrefix.count < $1.bundleIdPrefix.count }?
            .level
    }
}

/// What reading the settings file found (see `StoredFileRead`).
public typealias SettingsRead = StoredFileRead<JarvisSettings>

/// `JarvisSettings` in one JSON file. The settings are the owner's safety rules, so a read
/// reports which case it was (`read()`): a missing file means the defaults; an unreadable
/// one (before the first unlock, an I/O error) is never overwritten; a damaged one is set
/// aside. Writes are atomic and protected like `FileMemoryStore`.
public struct SettingsStore: Sendable {
    public let fileURL: URL
    private let now: @Sendable () -> Date

    public init(fileURL: URL, now: @escaping @Sendable () -> Date = Date.init) {
        self.fileURL = fileURL
        self.now = now
    }

    /// Reads the file. A damaged file (not a JSON object the settings decode from) is
    /// renamed to `settings.json.corrupt-<ms>` and reported with its new URL.
    public func read() -> SettingsRead {
        StoredFile.read(fileURL, now: now) { try JSONDecoder().decode(JarvisSettings.self, from: $0) }
    }

    /// The settings, or the defaults when there are none to use. Nothing that runs a task
    /// may use this: it cannot tell "no file" from "could not read the file". Use `read()`
    /// (or `SettingsGate`).
    public func load() -> JarvisSettings {
        read().value(or: JarvisSettings())
    }

    /// Writes `settings` atomically. Refused (`StoredFileError`, nothing changes on disk)
    /// while the existing file cannot be read, or is damaged and could not be set aside.
    public func save(_ settings: JarvisSettings) throws {
        try StoredFile.requireWritable(read(), fileURL)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try StoredFile.write(try encoder.encode(settings), to: fileURL)
    }
}

/// Why the owner's rules cannot be used yet.
public enum SettingsProblem: Sendable, Equatable {
    /// The file is there but could not be read (before the first unlock after a restart,
    /// or an I/O error). Tasks wait until it can be read.
    case unreadable(reason: String)
    /// The file was damaged and set aside (`setAside`, `nil` when it could not be moved):
    /// the defaults are in effect, and tasks wait until the owner has reviewed the rules.
    case damaged(reason: String, setAside: URL?)

    /// Plain words for the owner (also the text of `TaskCenter.Refusal.settingsUnavailable`).
    public var ownerMessage: String {
        switch self {
        case .unreadable:
            return "Jarvis cannot read your rules yet. Unlock the phone and try again."
        case .damaged(_, let setAside):
            let kept = setAside.map { " It was kept as \($0.lastPathComponent); nothing was deleted." }
                ?? " It could not be moved aside, so Jarvis leaves it as it is."
            return "Your rules file was damaged and has been set aside." + kept
                + " Your rules are back to the defaults until you review them. Check them, then confirm you have reviewed them."
        }
    }

    /// The technical reason (for a details line).
    public var reason: String {
        switch self {
        case .unreadable(let reason), .damaged(let reason, _): reason
        }
    }
}

/// The owner's rules as the app runs them: reads them through `SettingsStore.read()`, keeps
/// why they cannot be used (`problem`) and feeds `TaskCenter` (`settings` and
/// `settingsProblem`), so no task runs under defaults because the rules were unreadable or
/// damaged.
///
/// - Unreadable: `settings` are the defaults (shown, never saved: the store refuses), and
///   `refresh()` reads again (call it when the app becomes active and when protected data
///   becomes available). Once readable, the file's rules are used and the problem is gone.
/// - Damaged: the file was set aside; the defaults are in effect, and the problem stays
///   until `acknowledgeReview()`.
@MainActor
public final class SettingsGate {
    public let store: SettingsStore
    public private(set) var settings: JarvisSettings
    public private(set) var problem: SettingsProblem?

    public init(store: SettingsStore) {
        self.store = store
        self.settings = JarvisSettings()
        self.problem = nil
        apply(store.read())
    }

    /// What `TaskCenter` refuses with, `nil` when tasks may start.
    public var refusal: String? { problem?.ownerMessage }

    /// Reads the file again while it could not be read. Returns whether `settings` or
    /// `problem` changed. A damaged file waits for `acknowledgeReview()` instead.
    @discardableResult
    public func refresh() -> Bool {
        guard case .unreadable = problem else { return false }
        let before = (settings, problem)
        apply(store.read())
        return before.0 != settings || before.1 != problem
    }

    /// The owner reviewed the rules after a damaged file was set aside: tasks may run again
    /// under the rules as they now are. No effect in any other state.
    public func acknowledgeReview() {
        guard case .damaged = problem else { return }
        problem = nil
    }

    /// Saves `settings` (the owner changed them). Refused while the file cannot be read; the
    /// rules on disk stay as they were.
    public func save(_ new: JarvisSettings) throws {
        try store.save(new)
        settings = new
    }

    private func apply(_ read: SettingsRead) {
        switch read {
        case .loaded(let loaded):
            settings = loaded
            problem = nil
        case .missing:
            settings = JarvisSettings()
            problem = nil
        case .unreadable(let reason):
            settings = JarvisSettings()
            problem = .unreadable(reason: reason)
        case .damaged(let reason, let setAside):
            settings = JarvisSettings()
            problem = .damaged(reason: reason, setAside: setAside)
        }
    }
}
