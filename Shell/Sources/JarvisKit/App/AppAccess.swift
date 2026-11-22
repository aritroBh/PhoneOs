import Foundation
import JarvisCore

/// One app on the Apps screen: the ways Jarvis can get into it (PLAN §2.3 / §2.7), from
/// what this build really has, and the owner's trust level.
public struct AppAccess: Sendable, Equatable, Identifiable {
    /// The three ways into an app, cheapest first in the router, listed in screen order.
    public enum Way: String, Sendable, CaseIterable {
        /// Through the screen, by the runner (any app the policy lets Jarvis open).
        case screen
        /// App actions: registered tools of the Shortcuts / App Intents tier for this app.
        case appActions
        /// Direct: registered native tools (system frameworks) for this app.
        case direct

        public var title: String {
            switch self {
            case .screen: "Screen"
            case .appActions: "App actions"
            case .direct: "Direct"
            }
        }
    }

    public var bundleId: String
    public var name: String
    /// The owner's level (longest matching prefix), `nil` when none is set: the core then
    /// treats the app as Guarded and asks the first time.
    public var trust: TrustLevelOption?
    /// The core's own deny list covers this app (never opened, whatever the settings say).
    public var deniedByCore: Bool
    /// Names of registered tools that act in this app as app actions.
    public var appActions: [String]
    /// Names of registered native tools that act in this app.
    public var directTools: [String]

    public var id: String { bundleId }

    /// Whether the screen way is open: not denied by the owner or the core.
    public var screen: Bool { trust != .denied && !deniedByCore }

    public func has(_ way: Way) -> Bool {
        switch way {
        case .screen: screen
        case .appActions: !appActions.isEmpty && screen
        case .direct: !directTools.isEmpty && screen
        }
    }

    /// The open ways, for VoiceOver ("Ways in: screen, app actions").
    public var waysDescription: String {
        let open = Way.allCases.filter(has).map { $0.title.lowercased() }
        return open.isEmpty ? "No way in" : "Ways in: " + open.joined(separator: ", ")
    }
}

/// Builds the Apps list from data the app really has: the owner's trust list, the apps the
/// agent knows by name, and the registered tools that declare an app. Nothing is invented:
/// an app shows "app actions" or "direct" only when such a tool is registered.
public enum AppAccessCatalog {
    public static func make(
        settings: JarvisSettings,
        knownApps: [AppInfo],
        tools: [ToolSpec],
        coreDeniedApps: [String] = []
    ) -> [AppAccess] {
        var order: [String] = []
        var names: [String: String] = [:]
        func add(_ id: String, _ name: String?) {
            let id = id.trimmingCharacters(in: .whitespaces)
            guard !id.isEmpty else { return }
            if !order.contains(id) { order.append(id) }
            if names[id] == nil, let name, !name.isEmpty { names[id] = name }
        }
        for trust in settings.appTrust { add(trust.bundleIdPrefix, trust.name) }
        for app in knownApps { add(app.bundleId, app.name) }
        for tool in tools { if let app = tool.app { add(app, nil) } }

        func matches(_ bundleId: String, _ prefix: String) -> Bool {
            bundleId == prefix || bundleId.hasPrefix(prefix + ".")
        }
        return order.map { id in
            let own = tools.filter { $0.app.map { matches($0, id) || matches(id, $0) } ?? false }
            return AppAccess(
                bundleId: id,
                name: names[id] ?? id,
                trust: settings.trust(for: id),
                deniedByCore: coreDeniedApps.contains { matches(id, $0) },
                appActions: own.filter { $0.tier == .shortcut }.map(\.name),
                directTools: own.filter { $0.tier == .native }.map(\.name)
            )
        }
    }
}
