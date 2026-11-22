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
/// (missing fields take the defaults), so older files keep loading.
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
        modelBackend: ModelBackendKind = .openAICompatible,
        modelBaseURL: String = "http://127.0.0.1:8080",
        modelName: String = "local-model",
        modelContextTokens: Int = 8_192,
        runnerBaseURL: String = "http://127.0.0.1:8300",
        appTrust: [AppTrust] = JarvisSettings.defaultAppTrust
    ) {
        self.policyMode = policyMode
        self.firstUseApproval = firstUseApproval
        self.relaxedCategories = relaxedCategories
        self.denyCategories = denyCategories
        self.maxSteps = maxSteps
        self.maxMinutes = maxMinutes
        self.modelBackend = modelBackend
        self.modelBaseURL = modelBaseURL
        self.modelName = modelName
        self.modelContextTokens = modelContextTokens
        self.runnerBaseURL = runnerBaseURL
        self.appTrust = appTrust
    }

    enum CodingKeys: String, CodingKey {
        case policyMode = "policy_mode"
        case firstUseApproval = "first_use_approval"
        case relaxedCategories = "relaxed_categories"
        case denyCategories = "deny_categories"
        case maxSteps = "max_steps"
        case maxMinutes = "max_minutes"
        case modelBackend = "model_backend"
        case modelBaseURL = "model_base_url"
        case modelName = "model_name"
        case modelContextTokens = "model_context_tokens"
        case runnerBaseURL = "runner_base_url"
        case appTrust = "app_trust"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = JarvisSettings()
        policyMode = (try? c.decodeIfPresent(PolicyModeOption.self, forKey: .policyMode)) ?? d.policyMode
        firstUseApproval = (try? c.decodeIfPresent(Bool.self, forKey: .firstUseApproval)) ?? d.firstUseApproval
        relaxedCategories = Self.lenientList(c, .relaxedCategories) ?? d.relaxedCategories
        denyCategories = Self.lenientList(c, .denyCategories) ?? d.denyCategories
        maxSteps = (try? c.decodeIfPresent(Int.self, forKey: .maxSteps)) ?? d.maxSteps
        maxMinutes = (try? c.decodeIfPresent(Int.self, forKey: .maxMinutes)) ?? d.maxMinutes
        modelBackend = (try? c.decodeIfPresent(ModelBackendKind.self, forKey: .modelBackend)) ?? d.modelBackend
        modelBaseURL = (try? c.decodeIfPresent(String.self, forKey: .modelBaseURL)) ?? d.modelBaseURL
        modelName = (try? c.decodeIfPresent(String.self, forKey: .modelName)) ?? d.modelName
        modelContextTokens = (try? c.decodeIfPresent(Int.self, forKey: .modelContextTokens)) ?? d.modelContextTokens
        runnerBaseURL = (try? c.decodeIfPresent(String.self, forKey: .runnerBaseURL)) ?? d.runnerBaseURL
        appTrust = (try? c.decodeIfPresent([AppTrust].self, forKey: .appTrust)) ?? d.appTrust
    }

    /// Decodes a category list, skipping names this build does not know.
    private static func lenientList(_ c: KeyedDecodingContainer<CodingKeys>, _ key: CodingKeys) -> [RiskCategoryOption]? {
        guard let names = try? c.decodeIfPresent([String].self, forKey: key) else { return nil }
        return names.compactMap(RiskCategoryOption.init(rawValue:))
    }

    // MARK: - Validated values

    /// `maxSteps` clamped to 1…500.
    public var clampedMaxSteps: UInt32 { UInt32(min(max(maxSteps, 1), 500)) }
    /// `maxMinutes` clamped to 1…120, in milliseconds.
    public var maxDurationMs: UInt64 { UInt64(min(max(maxMinutes, 1), 120)) * 60_000 }

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

/// `JarvisSettings` in one JSON file. Reads never fail (a missing or unreadable file gives
/// the defaults); writes are atomic.
public struct SettingsStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL) {
        self.fileURL = fileURL
    }

    public func load() -> JarvisSettings {
        guard let data = try? Data(contentsOf: fileURL),
              let settings = try? JSONDecoder().decode(JarvisSettings.self, from: data) else {
            return JarvisSettings()
        }
        return settings
    }

    public func save(_ settings: JarvisSettings) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(settings)
        try FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: fileURL, options: [.atomic])
    }
}
