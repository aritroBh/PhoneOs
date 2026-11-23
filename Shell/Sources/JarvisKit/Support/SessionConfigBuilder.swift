import Foundation
import JarvisCore

/// Builds the core's `SessionConfig` from its own defaults.
///
/// Never construct `Policy` or `SessionConfig` with the generated memberwise initialisers:
/// the core's policy types gain fields, and starting from `defaultSessionConfig()` keeps
/// this code compiling and the new fields at their defaults.
public enum SessionConfigBuilder {
    /// The default config with the model profile from `capabilities`, the given tools,
    /// known apps, the agent's own bundle id (exempt from first-use approval), the skill
    /// the session runs under (`nil`: none; it can only narrow the policy), and
    /// `adjustPolicy` applied to the default policy (deny list, budgets). `driveOwnApp`
    /// (sixteenth review): whether the run drives Jarvis's own app (`agentBundleId`); off,
    /// the model gets a one-line stub for Jarvis's own screens and cannot act there. Only
    /// the simulator UI tests (the Playground) turn it on.
    public static func make(
        capabilities: ModelCapabilities,
        tools: [ToolSpec] = [],
        knownApps: [AppInfo] = [],
        agentBundleId: String? = nil,
        skill: SkillContext? = nil,
        driveOwnApp: Bool = false,
        adjustPolicy: (inout Policy) -> Void = { _ in }
    ) throws -> SessionConfig {
        var config = try defaultSessionConfig()
        config.model = try capabilities.modelProfile()
        config.tools = tools
        config.knownApps = knownApps
        if let agentBundleId { config.agentBundleId = agentBundleId }
        config.driveOwnApp = driveOwnApp
        config.skill = skill
        adjustPolicy(&config.policy)
        return config
    }
}
