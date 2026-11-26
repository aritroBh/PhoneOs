import Foundation
import JarvisCore

/// A system surface SpringBoard draws over an app (or the Home Screen) while the app below
/// stays in the foreground: Control Centre, Notification Centre (the cover sheet pulled down
/// on an unlocked phone), the App Switcher (docs/ARCHITECTURE.md, *Swift shell*, the top
/// layer).
///
/// What real trees showed (Mac run 139, `testP12a_BadgeAndSystemSurfaces`, iOS 26.5): with
/// each surface up over Settings, Settings still read as in front (`frontBundleId()` was
/// `com.apple.Preferences`), its full tree was still reported, and the surface was in
/// SpringBoard's tree only. A tap planned on Settings would have landed on a Control Centre
/// toggle, a notification row or a switcher card (a swipe up on a card closes an app).
///
/// Each surface is recognised by one identifier of SpringBoard's tree that is in every
/// capture of that surface and in no other capture of the corpus (the 21 captures of runs
/// 134, 135 and 137 in `RealSpringBoardTrees`, and the 21 SpringBoard captures of run 139:
/// the Home Screen, apps in front with pickers, sheets, menus, browser views and JavaScript
/// dialogs, eight system alerts, a banner over an app, Spotlight, the lock screen):
/// - Control Centre: `cc-root-folder-view` (the module grid's root).
/// - Notification Centre: `SBCoverSheetWindow` (the cover sheet's window) without the lock
///   screen's signature (`LockScreenSignature`): the window is also the lock screen's.
/// - the App Switcher: `handoff-banner`. Not `SBSwitcherWindow:Main`, `AppSwitcherContentView`
///   or a `card:<bundle>:sceneID:…` node: SpringBoard's tree holds all three, with the front
///   app's one card at the screen's frame, in every capture taken with an app in front.
///
/// The runner learns that one may be up from one existence query on SpringBoard (these
/// identifiers, the banner's and an alert's element type in one predicate: `SystemLayers`),
/// reads SpringBoard's tree only when it says yes, and decides here.
public enum SystemSurface: String, Sendable, Equatable, CaseIterable {
    case controlCentre
    case notificationCentre
    case appSwitcher
    /// Run 146 (P15): states of SpringBoard itself that are not the plain Home Screen,
    /// although its tree still holds the Home Screen's icon container (the runner read them
    /// as the Home Screen): the Home Screen's edit mode ("jiggle mode": a `DeleteButton` on
    /// every icon), the App Library and an open folder. Reported as the unidentified layer,
    /// named, like the surfaces; Home leaves each of them.
    case homeEditMode
    case appLibrary
    case openFolder

    /// The OS identifier that says the surface is up (see the type's comment for the evidence;
    /// for `openFolder` the end of one: `identifierIsSuffix`).
    public var identifier: String {
        switch self {
        case .controlCentre: return "cc-root-folder-view"
        case .notificationCentre: return "SBCoverSheetWindow"
        case .appSwitcher: return "handoff-banner"
        // P15: on every icon's delete badge in edit mode; in no other capture.
        case .homeEditMode: return "DeleteButton"
        // P15: the App Library's pod container (beside `dewey-search-field`); in no other capture.
        case .appLibrary: return "dewey-pod-view"
        // P15: the open folder's name field is `TextField id="<name>, Folder name"`.
        case .openFolder: return SystemSurface.folderNameSuffix
        }
    }

    /// Whether `identifier` is matched as the end of a node's identifier (the folder's name
    /// field carries the folder's name before it).
    public var identifierIsSuffix: Bool { self == .openFolder }

    /// The end of an open folder's name field's identifier (P15: `Utilities, Folder name`).
    public static let folderNameSuffix = ", Folder name"

    /// Whether a node's `identifier` says this surface is up.
    public func matches(_ identifier: String) -> Bool {
        identifierIsSuffix ? identifier.hasSuffix(self.identifier) : identifier == self.identifier
    }

    /// The surface's name, for notes and refusals.
    public var name: String {
        switch self {
        case .controlCentre: return "Control Centre"
        case .notificationCentre: return "Notification Centre"
        case .appSwitcher: return "the App Switcher"
        case .homeEditMode: return "the Home Screen's edit mode"
        case .appLibrary: return "the App Library"
        case .openFolder: return "an open folder on the Home Screen"
        }
    }

    /// The observation note: which surface is up over which app (`over`: the foreground
    /// reading, `nil` for SpringBoard). Nothing of the surface's content (no notification
    /// text, no control names, no folder name).
    public func note(over: String?) -> String {
        let tail = ": this observation holds no screen content, only the application node "
        switch self {
        case .homeEditMode:
            return "the Home Screen is in edit mode (its icons can be deleted or moved)\(tail)"
                + "(nothing of it is read; Home leaves it)"
        case .appLibrary:
            return "the App Library is up (not the Home Screen)\(tail)(nothing of it is read; Home leaves it)"
        case .openFolder:
            return "a folder is open on the Home Screen\(tail)(nothing of it is read; Home leaves it)"
        case .controlCentre, .notificationCentre, .appSwitcher:
            let head = name.prefix(1).uppercased() + name.dropFirst()
            let place = over.map { " over \($0)" } ?? ""
            return "\(head) is up\(place)\(tail)(nothing of \(name) is read; Home dismisses it)"
        }
    }

    /// The surface up in SpringBoard's raw tree, if any. Hidden nodes count, as for the Home
    /// Screen's identifier (`FrontApp.homeScreenShowing`).
    public static func up(inSpringBoard raw: RawElement) -> SystemSurface? {
        var present: Set<String> = []
        var stack = [raw]
        while let e = stack.popLast() {
            for s in allCases where s.matches(e.identifier) { present.insert(s.identifier) }
            stack.append(contentsOf: e.children)
        }
        return up(present: present, lockScreen: LockScreenSignature.matches(raw))
    }

    /// The decision on the identifiers present (`present`) and whether SpringBoard's tree
    /// shows the lock screen (`lockScreen`): Control Centre, else the App Switcher, else the
    /// cover sheet without the lock screen (Notification Centre), else the Home Screen's edit
    /// mode, the App Library, an open folder (edit mode first: deleting an app is the
    /// riskiest thing these offer). The cover sheet with the lock screen is no surface here:
    /// the lock is `LockJudge`'s.
    public static func up(present: Set<String>, lockScreen: Bool) -> SystemSurface? {
        if present.contains(SystemSurface.controlCentre.identifier) { return .controlCentre }
        if present.contains(SystemSurface.appSwitcher.identifier) { return .appSwitcher }
        if present.contains(SystemSurface.notificationCentre.identifier), !lockScreen { return .notificationCentre }
        for s in [SystemSurface.homeEditMode, .appLibrary, .openFolder] where present.contains(s.identifier) {
            return s
        }
        return nil
    }

    /// The check next to a touch on a layer Jarvis does not read (`FrontApp.unidentified`):
    /// the command was planned on such a layer (`expect`) while `planned` was up (the
    /// surface the last observation reported, `nil` for an unidentified app) and the layer
    /// now (`top`) is such a layer with `now` up. A different surface (or one where the look
    /// had none, or none where it had one) is stale: nothing is carried out. Same bundle id
    /// both ways, so `FrontAppGuard` cannot tell these apart. `nil`: no refusal.
    public static func staleRefusal(expect: CommandExpect?, top: String,
                                    planned: SystemSurface?, now: SystemSurface?) -> RunnerResult? {
        guard let expect, FrontApp.isUnidentified(expect.bundleId), FrontApp.isUnidentified(top),
              planned != now else { return nil }
        let was = planned?.name ?? "an app Jarvis cannot identify"
        let isNow = now.map { "\($0.name) is up" } ?? "no system surface is up"
        return RunnerResult(
            ok: false,
            error: "not carried out: the command was planned on \(was), and now \(isNow)",
            failure: .stale
        )
    }
}

/// What SpringBoard draws over the foreground reading, from one reading of its tree: an
/// alert, a system surface, banners. The runner asks one existence query
/// (`triggerIdentifiers` and the alert element type) and reads SpringBoard's tree only when
/// it answers yes (or reuses the tree the observation already took).
public struct SystemLayers: Sendable, Equatable {
    /// An alert is in SpringBoard's tree.
    public var alert: Bool
    /// A system surface is up (`SystemSurface.up`).
    public var surface: SystemSurface?
    /// The notification banners' frames in screen points (`BannerCover.frames`); empty
    /// while a surface is up (Notification Centre's rows have the banner's identifier, and
    /// the surface decides the layer).
    public var banners: [Rect]

    public init(alert: Bool = false, surface: SystemSurface? = nil, banners: [Rect] = []) {
        self.alert = alert
        self.surface = surface
        self.banners = banners
    }

    /// Nothing up (the existence query answered no).
    public static let nothing = SystemLayers()

    /// The identifiers of the existence query (`identifier == …`), with the alert element type
    /// (`XCUIElementTypeCode.alert`): the banner's and each surface's matched whole.
    public static let triggerIdentifiers: [String] = [BannerCover.identifier]
        + SystemSurface.allCases.filter { !$0.identifierIsSuffix }.map(\.identifier)
    /// The identifier ends of the same existence query (`identifier ENDSWITH …`): the open
    /// folder's (run 146, P15). Still one query: more terms of its one predicate.
    public static let triggerSuffixes: [String] = SystemSurface.allCases.filter(\.identifierIsSuffix).map(\.identifier)

    /// Everything read from SpringBoard's raw tree.
    public static func read(springBoard raw: RawElement) -> SystemLayers {
        var alert = false
        var stack = [raw]
        while let e = stack.popLast() {
            if e.elementType == XCUIElementTypeCode.alert {
                alert = true
                break
            }
            stack.append(contentsOf: e.children)
        }
        let surface = SystemSurface.up(inSpringBoard: raw)
        return SystemLayers(alert: alert, surface: surface,
                            banners: surface == nil ? BannerCover.frames(inSpringBoard: raw) : [])
    }
}
