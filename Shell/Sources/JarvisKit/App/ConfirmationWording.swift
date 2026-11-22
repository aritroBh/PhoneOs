import Foundation
import JarvisCore

/// How a confirmation reads to the owner. The core words its requests for the log and the
/// model; the sheet needs a question. The one case that cannot use the app name is a tap on
/// a home-screen icon the core cannot name (`FirstUse`, app `com.apple.springboard`, summary
/// `Let Jarvis open 'Weather' from the home screen? It is not one of the known apps.`): there
/// the question names the icon, not SpringBoard.
public struct ConfirmationWording: Sendable, Equatable {
    /// The big question on the sheet.
    public var title: String
    /// The chip above it.
    public var chip: String
    /// Set for a home-screen launch: what the owner is being asked to let Jarvis open.
    public var homeScreenTarget: String?

    /// SpringBoard's bundle id, the `app` of a home-screen launch request.
    public static let springboard = "com.apple.springboard"

    /// - Parameter appName: the app's display name, as the sheet shows it today.
    public init(_ request: ConfirmRequest, appName: String) {
        if request.category == .firstUse, request.app == Self.springboard,
           let target = Self.homeScreenTarget(in: request.summary) {
            homeScreenTarget = target
            title = "Open \(target) from the Home Screen?"
            chip = "Home Screen"
        } else {
            homeScreenTarget = nil
            title = request.category == .firstUse ? "Let Jarvis act in \(appName)?" : "Go ahead in \(appName)?"
            chip = riskCategoryTitle(request.category)
        }
    }

    /// `"Weather"` (quoted) for an icon, `the top search result for "x"` for the search
    /// field's Return; `nil` when the summary is not a home-screen launch request.
    static func homeScreenTarget(in summary: String) -> String? {
        let prefix = "Let Jarvis open "
        guard summary.hasPrefix(prefix), let end = summary.range(of: " from the home screen?") else { return nil }
        let what = String(summary[summary.index(summary.startIndex, offsetBy: prefix.count)..<end.lowerBound])
        guard !what.isEmpty else { return nil }
        // The core writes quotes inside text as `'`; show them as the owner's quotes.
        return what.replacingOccurrences(of: "'", with: "\"")
    }
}
