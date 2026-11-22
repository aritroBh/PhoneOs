import Foundation
import JarvisCore

/// Builds the core's `SessionConfig` from its own defaults.
///
/// Never construct `Policy` or `SessionConfig` with the generated memberwise initialisers:
/// the core's policy types gain fields, and starting from `defaultSessionConfig()` keeps
/// this code compiling and the new fields at their defaults.
public enum SessionConfigBuilder {
    /// The default config with the model profile from `capabilities`, the given tools,
    /// known apps and the agent's own bundle id (exempt from first-use approval), and
    /// `adjustPolicy` applied to the default policy (deny list, budgets).
    public static func make(
        capabilities: ModelCapabilities,
        tools: [ToolSpec] = [],
        knownApps: [AppInfo] = [],
        agentBundleId: String? = nil,
        adjustPolicy: (inout Policy) -> Void = { _ in }
    ) throws -> SessionConfig {
        var config = try defaultSessionConfig()
        config.model = try capabilities.modelProfile()
        config.tools = tools
        config.knownApps = knownApps
        if let agentBundleId { config.agentBundleId = agentBundleId }
        adjustPolicy(&config.policy)
        return config
    }
}
