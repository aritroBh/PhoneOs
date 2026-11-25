import Foundation
import JarvisCore

/// Which app is in front, as the in-process XCUITest runner (`XCUITestPhoneRunner`, Mac only)
/// decides it: the pure parts, tested on Linux (docs/ARCHITECTURE.md, *Swift shell*, the
/// front-app rule).
///
/// XCUITest has no public way to ask which app is in front. The runner used to check a fixed
/// list of twelve bundle ids and read anything else as SpringBoard, whose tree holds no icons
/// while an app is in front: the core then saw an empty Home Screen, an element-less swipe
/// classified as harmless, the front-app guard passed (SpringBoard == SpringBoard), and the
/// gesture landed on the real app unasked. Two parts now:
///
/// 1. **Fail closed** (public API, the safety net): when the foreground reading is
///    SpringBoard, no SpringBoard alert and no Spotlight is up and the phone is not locked,
///    SpringBoard must show its Home Screen (`homeScreenShowing`); otherwise an app Jarvis
///    cannot identify is in front, reported as `unidentified` with a tree of the application
///    node only (`emptyRaw`) and `unidentifiedNote`.
/// 2. **Identify it** (the runner's `FrontProcess`, private API through the Objective-C
///    runtime, degrading to part 1): the accessibility client's active applications, chosen
///    among by `choose`.
public enum FrontApp {
    /// The synthetic top layer for an app in front that the runner cannot identify. Not a
    /// bundle id; mirrored in the core (`jarvis_proto::UNIDENTIFIED_APP_BUNDLE_ID`), which
    /// treats its screen as one Jarvis cannot read and never as SpringBoard, Jarvis's own
    /// app, a known app or something to launch.
    public static let unidentified = "unknown.front.app"

    /// The observation note for `unidentified`.
    public static let unidentifiedNote =
        "an app Jarvis cannot identify is in front (SpringBoard does not show its Home Screen, and no "
        + "known app is in front): this observation holds no screen content, only the application node"

    /// The identifier of the Home Screen's icon container in SpringBoard's tree. iOS 26.5
    /// simulator: present in every Home Screen capture of the corpus (pages 1 and 2, before
    /// the agent tests launched Settings and Safari, and under Spotlight), absent from every
    /// SpringBoard capture taken while an app was in front (Jarvis with a share sheet, an
    /// edit or context menu, `SFSafariViewController`, a web sign-in sheet; Settings under a
    /// banner), from the lock screen and from the system alerts.
    public static let homeScreenIdentifier = "Home screen icons"

    /// The identifier namespace of the Home Screen's icon menu (`com.apple.springboardhome.`
    /// `application-shortcut-item.rearrange-icons`, `…remove-app`): with an icon's menu up
    /// SpringBoard's tree holds the menu only, no `homeScreenIdentifier`
    /// (`fact-home-icon-menu`, runs 134, 135, 137).
    public static let homeScreenIdentifierPrefix = "com.apple.springboardhome."

    /// Whether SpringBoard's raw tree shows its Home Screen: some node's identifier is
    /// `homeScreenIdentifier`, or starts with `homeScreenIdentifierPrefix`. Hidden nodes count
    /// (the snapshot holds what SpringBoard draws; an app in front leaves none of these).
    public static func homeScreenShowing(_ springBoard: RawElement) -> Bool {
        var stack = [springBoard]
        while let e = stack.popLast() {
            if isHomeScreenIdentifier(e.identifier) { return true }
            stack.append(contentsOf: e.children)
        }
        return false
    }

    /// Whether `identifier` is one of the Home Screen's (`homeScreenShowing`).
    public static func isHomeScreenIdentifier(_ identifier: String) -> Bool {
        identifier == homeScreenIdentifier || identifier.hasPrefix(homeScreenIdentifierPrefix)
    }

    /// Whether `bundleId` is `unidentified` (case and surrounding spaces ignored).
    public static func isUnidentified(_ bundleId: String) -> Bool {
        bundleId.trimmingCharacters(in: .whitespaces).caseInsensitiveCompare(unidentified) == .orderedSame
    }

    /// The raw tree of an observation that may show nothing (`unidentified`, a locked
    /// phone): the application element only, at the screen's frame, with no label, value or
    /// identifier. `UiTreeBuilder.build` turns it into the application node alone.
    public static func emptyRaw(screen: Rect) -> RawElement {
        RawElement(elementType: XCUIElementTypeCode.application, frame: screen)
    }

    /// What the private path's reading came to (`choose`).
    public struct Choice: Sendable, Equatable {
        /// The app in front; SpringBoard when only SpringBoard (and processes that are never
        /// the front app) is active; `nil` when the reading does not tell (nothing active,
        /// an active process whose bundle id was not read, several candidates).
        public var bundleId: String?
        /// Why, for the observation notes.
        public var why: String

        public init(bundleId: String?, why: String) {
            self.bundleId = bundleId
            self.why = why
        }
    }

    /// Chooses the front app from the accessibility client's active applications (`active`:
    /// each one's bundle id, `nil` when it could not be read), in the client's order.
    /// SpringBoard, Spotlight (decided by the top-layer rule), the test runner itself
    /// (`runner`) and `unidentified` are never the front app. One candidate left: it. None:
    /// SpringBoard when it is active, else `nil`. Several: the one `isForeground` holds for,
    /// when exactly one does, else `nil`. An active process whose bundle id was not read
    /// makes the reading tell nothing (`nil`: it may be the front app).
    public static func choose(
        active: [String?],
        runner: String?,
        isForeground: (String) -> Bool
    ) -> Choice {
        if active.isEmpty { return Choice(bundleId: nil, why: "no active application") }
        if active.contains(where: { ($0?.trimmingCharacters(in: .whitespaces) ?? "").isEmpty }) {
            return Choice(bundleId: nil, why: "an active application's bundle id could not be read")
        }
        var names: [String] = []
        for case let id? in active {
            let t = id.trimmingCharacters(in: .whitespaces)
            if !names.contains(t) { names.append(t) }
        }
        let own = runner?.trimmingCharacters(in: .whitespaces) ?? ""
        let candidates = names.filter { id in
            !TopLayer.isSpringBoard(id)
                && id.caseInsensitiveCompare(TopLayer.spotlight) != .orderedSame
                && !isUnidentified(id)
                && (own.isEmpty || id != own)
        }
        let list = names.joined(separator: ", ")
        if candidates.count == 1 {
            return Choice(bundleId: candidates[0], why: "active applications: \(list)")
        }
        if candidates.isEmpty {
            if names.contains(where: TopLayer.isSpringBoard) {
                return Choice(bundleId: TopLayer.springBoard, why: "active applications: \(list) (no app over SpringBoard)")
            }
            return Choice(bundleId: nil, why: "active applications: \(list) (none is SpringBoard or an app)")
        }
        let front = candidates.filter(isForeground)
        if front.count == 1 {
            return Choice(bundleId: front[0], why: "active applications: \(list); the one in the foreground: \(front[0])")
        }
        return Choice(bundleId: nil, why: "active applications: \(list); \(front.count) of them in the foreground: not decided")
    }
}
