import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// System surfaces drawn by SpringBoard over an app (`SystemSurface`, `SystemLayers`,
/// `TopLayer.decide`, the lock interplay in `LockJudge`): the pure decisions the Mac-only
/// XCUITest runner makes, on every real SpringBoard tree of runs 134, 135, 137
/// (`RealSpringBoardTrees`) and 139 (`RealSystemSurfaceTrees`).
@Suite("System surfaces: Control Centre, Notification Centre, the App Switcher (SystemSurface)")
struct SystemSurfaceTests {
    let settings = "com.apple.Preferences"

    /// One capture of the whole corpus: its name, what it shows (the surface up, if any) and
    /// its tree.
    struct Sample {
        let name: String
        let surface: SystemSurface?
        /// The capture is of the lock screen (run 137).
        let lock: Bool
        /// The capture holds a SpringBoard alert.
        let alert: Bool
        /// An app was in front (SpringBoard's tree taken while it was).
        let app: Bool
        let root: RawElement
    }

    private func corpus() throws -> [Sample] {
        var out: [Sample] = []
        for c in RealSpringBoardTrees.all {
            out.append(Sample(name: c.name, surface: nil, lock: c.kind == .lock, alert: c.kind == .alert,
                              app: c.kind == .app, root: try RawTreeText.parse(c.text)))
        }
        for c in RealSystemSurfaceTrees.all {
            let surface: SystemSurface?
            switch c.kind {
            case .controlCentre: surface = .controlCentre
            case .notificationCentre: surface = .notificationCentre
            case .appSwitcher: surface = .appSwitcher
            case .home, .app, .alert: surface = nil
            }
            out.append(Sample(name: "run139." + c.name, surface: surface, lock: false, alert: c.kind == .alert,
                              app: c.kind == .app, root: try RawTreeText.parse(c.text)))
        }
        return out
    }

    private func count(_ root: RawElement, _ test: (RawElement) -> Bool) -> Int {
        var n = 0
        var stack = [root]
        while let e = stack.popLast() {
            if test(e) { n += 1 }
            stack.append(contentsOf: e.children)
        }
        return n
    }

    private func has(_ root: RawElement, id: String) -> Bool {
        count(root) { $0.identifier == id } > 0
    }

    @Test("the parser reads every run-139 capture whole: SpringBoard's application element, every line a node")
    func parser() throws {
        for c in RealSystemSurfaceTrees.all {
            let root = try RawTreeText.parse(c.text)
            #expect(root.elementType == XCUIElementTypeCode.application, "\(c.name)")
            #expect(root.count == c.text.split(separator: "\n").count, "\(c.name)")
            let ids = c.text.components(separatedBy: " id=").count - 1
            let labels = c.text.components(separatedBy: " label=\"").count - 1
            #expect(count(root) { !$0.identifier.isEmpty } == ids, "\(c.name)")
            #expect(count(root) { !$0.label.isEmpty } == labels, "\(c.name)")
        }
        // The corpus: 21 captures of runs 134 to 137, 24 of run 139.
        #expect(RealSpringBoardTrees.all.count == 21)
        #expect(RealSystemSurfaceTrees.all.count == 24)
        let kinds = Dictionary(grouping: RealSystemSurfaceTrees.all, by: \.kind).mapValues(\.count)
        #expect(kinds[.controlCentre] == 1 && kinds[.notificationCentre] == 1 && kinds[.appSwitcher] == 1)
        #expect(kinds[.home] == 1 && kinds[.app] == 13 && kinds[.alert] == 7)
    }

    @Test("the evidence, identifier by identifier: each chosen one is in its surface's capture and in no other capture of the 45")
    func evidence() throws {
        let samples = try corpus()
        #expect(samples.count == 45)
        for s in samples {
            // Control Centre's module grid.
            #expect(has(s.root, id: "cc-root-folder-view") == (s.surface == .controlCentre), "\(s.name)")
            // The App Switcher's handoff banner container.
            #expect(has(s.root, id: "handoff-banner") == (s.surface == .appSwitcher), "\(s.name)")
            // The cover sheet's window: Notification Centre, and the lock screen.
            #expect(has(s.root, id: "SBCoverSheetWindow") == (s.surface == .notificationCentre || s.lock), "\(s.name)")
            // So are its notification rows and its date view (the cover sheet's own parts).
            for shared in ["ListCell", "lockscreen-date-view"] {
                #expect(has(s.root, id: shared) == (s.surface == .notificationCentre || s.lock), "\(s.name): \(shared)")
            }
            // Notification Centre's Clear button: only there (but only while it holds
            // notifications, so it is not the rule).
            #expect(has(s.root, id: "clear-button") == (s.surface == .notificationCentre), "\(s.name)")
            // What does NOT tell the App Switcher: its window, its content view and the cards
            // are in SpringBoard's tree with any app in front too (one card, at the screen's
            // frame).
            let switcherWindow = has(s.root, id: "SBSwitcherWindow:Main")
            let content = has(s.root, id: "AppSwitcherContentView")
            let cards = count(s.root) { $0.identifier.hasPrefix("card:") }
            if s.app {
                #expect(switcherWindow && content && cards == 1, "\(s.name)")
                let card = try #require(firstNode(s.root) { $0.identifier.hasPrefix("card:") })
                #expect(card.frame == s.root.frame, "\(s.name): the front app's card fills the screen")
            }
            if s.surface == .appSwitcher {
                #expect(switcherWindow && content && cards == 5, "\(s.name)")
            }
        }
    }

    private func firstNode(_ root: RawElement, _ test: (RawElement) -> Bool) -> RawElement? {
        var stack = [root]
        while let e = stack.popLast() {
            if test(e) { return e }
            stack.append(contentsOf: e.children.reversed())
        }
        return nil
    }

    @Test("the rule on every real tree: each surface is found in its capture, nothing in the other 42 (the lock screen is no surface)")
    func ruleOnRealTrees() throws {
        for s in try corpus() {
            #expect(SystemSurface.up(inSpringBoard: s.root) == s.surface, "\(s.name)")
            let layers = SystemLayers.read(springBoard: s.root)
            #expect(layers.surface == s.surface, "\(s.name)")
            #expect(layers.alert == s.alert, "\(s.name)")
            // The lock screen check is unchanged by the new captures.
            #expect(LockScreenSignature.matches(s.root) == s.lock, "\(s.name)")
            // A surface is not the Home Screen (Notification Centre and Control Centre were
            // pulled over Settings).
            if s.surface != nil { #expect(!FrontApp.homeScreenShowing(s.root), "\(s.name)") }
        }
    }

    @Test("the decision on identifiers alone: Control Centre, else the App Switcher, else the cover sheet without the lock screen")
    func identifierTable() {
        let cc = SystemSurface.controlCentre.identifier
        let nc = SystemSurface.notificationCentre.identifier
        let sw = SystemSurface.appSwitcher.identifier
        #expect(SystemSurface.up(present: [], lockScreen: false) == nil)
        #expect(SystemSurface.up(present: [cc], lockScreen: false) == .controlCentre)
        #expect(SystemSurface.up(present: [sw], lockScreen: false) == .appSwitcher)
        #expect(SystemSurface.up(present: [nc], lockScreen: false) == .notificationCentre)
        #expect(SystemSurface.up(present: [nc], lockScreen: true) == nil, "the cover sheet on a locked phone is the lock screen")
        #expect(SystemSurface.up(present: [cc, nc], lockScreen: true) == .controlCentre)
        #expect(SystemSurface.up(present: [sw, nc], lockScreen: false) == .appSwitcher)
        #expect(SystemSurface.up(present: ["SBSwitcherWindow:Main", "AppSwitcherContentView", "card:com.apple.Preferences:sceneID:x"], lockScreen: false) == nil)
        // The runner's one existence query asks for all of them and the banner.
        // Run 146 (P15): and the Home Screen's edit mode and the App Library (whole), the open
        // folder's name field (its end).
        #expect(Set(SystemLayers.triggerIdentifiers) == [BannerCover.identifier, cc, nc, sw, "DeleteButton", "dewey-pod-view"])
        #expect(SystemLayers.triggerSuffixes == [", Folder name"])
    }

    @Test("the top layer: alert, then system surface, then Spotlight, then the app; a surface is the unidentified layer, named")
    func precedence() {
        typealias D = TopLayer.Decision
        let sb = TopLayer.springBoard
        for surface in SystemSurface.allCases {
            let over = TopLayer.decide(systemAlertUp: false, systemSurface: surface, spotlightInFront: false,
                                       foreground: settings, springBoard: .other)
            #expect(over == D(bundleId: FrontApp.unidentified, surface: surface, surfaceOver: settings))
            #expect(FrontApp.isUnidentified(over.bundleId))
            #expect(over.note == surface.note(over: settings))
            // The surfaces over an app name it; SpringBoard's own states (run 146) are never over
            // an app and say Home leaves them.
            if [SystemSurface.controlCentre, .notificationCentre, .appSwitcher].contains(surface) {
                #expect(over.note?.lowercased().contains("\(surface.name.lowercased()) is up over \(settings.lowercased())") == true)
            } else {
                #expect(over.note?.contains("Home leaves it") == true)
            }
            // An alert over the surface is still an alert.
            #expect(TopLayer.decide(systemAlertUp: true, systemSurface: surface, spotlightInFront: false,
                                    foreground: settings, springBoard: .other) == D(bundleId: sb, alertOver: settings, systemAlert: true))
            // The surface is over Spotlight.
            #expect(TopLayer.decide(systemAlertUp: false, systemSurface: surface, spotlightInFront: true,
                                    foreground: sb, springBoard: .homeScreen) == D(bundleId: FrontApp.unidentified, surface: surface))
            // Over the Home Screen too (and whatever SpringBoard shows).
            for shows in [TopLayer.SpringBoardShows.homeScreen, .lockScreen, .other] {
                #expect(TopLayer.decide(systemAlertUp: false, systemSurface: surface, spotlightInFront: false,
                                        foreground: sb, springBoard: shows).bundleId == FrontApp.unidentified)
            }
        }
        // No surface: the decision of before.
        #expect(TopLayer.decide(systemAlertUp: false, systemSurface: nil, spotlightInFront: false, foreground: settings, springBoard: .other)
            == D(bundleId: settings))
    }

    @Test("the notes name the surface and the app below, never the surface's content")
    func notes() throws {
        let cc = SystemSurface.controlCentre.note(over: settings)
        #expect(cc.hasPrefix("Control Centre is up over com.apple.Preferences"))
        #expect(SystemSurface.appSwitcher.note(over: nil).hasPrefix("The App Switcher is up:"))
        #expect(SystemSurface.notificationCentre.note(over: settings).hasPrefix("Notification Centre is up over com.apple.Preferences"))
        // Nothing of what the real captures show reaches a note.
        let content = ["Probe banner 4417", "Ready for Apple Intelligence", "Jarvis probe", "Clear", "Add Controls", "Power",
                       "Wi-Fi", "Airplane", "Messages", "Safari", "Contacts"]
        for surface in SystemSurface.allCases {
            let note = surface.note(over: settings)
            for word in content { #expect(!note.contains(word), "\(surface): \(word)") }
        }
    }

    @Test("Notification Centre's rows are not banners: the surface decides, so the banner gate does not wait")
    func bannerGateComesAfter() throws {
        let nc = try RawTreeText.parse(RealSystemSurfaceTrees.notificationCentre.text)
        // The rows have the banner's identifier (2 of them in this capture) …
        #expect(BannerCover.frames(inSpringBoard: nc).count == 2)
        // … but the reading puts the surface first and reports no banner.
        let layers = SystemLayers.read(springBoard: nc)
        #expect(layers.surface == .notificationCentre && layers.banners.isEmpty)
        let tap = RunnerCommand.tap(x: 220, y: 620)
        #expect(BannerGate.mayProceed(tap, bannersUp: !layers.banners.isEmpty))
        // The tap planned on Settings is refused at once as stale by the top layer.
        let top = TopLayer.decide(systemAlertUp: layers.alert, systemSurface: layers.surface, spotlightInFront: false,
                                  foreground: settings, springBoard: .other)
        #expect(FrontAppGuard.refusalNextToGesture(CommandExpect(bundleId: settings), front: top.bundleId)?.failure == .stale)
        // A real banner over Settings is still a banner (the gate waits for it).
        let banner = SystemLayers.read(springBoard: try RawTreeText.parse(RealSpringBoardTrees.settingsBanner134.text))
        #expect(banner.surface == nil && banner.banners.count == 1 && !banner.alert)
    }

    @Test("touch time: planned on the app, a surface up is stale; planned on a surface, the app back or another surface is stale")
    func stale() {
        let onSettings = CommandExpect(bundleId: settings)
        let onLayer = CommandExpect(bundleId: FrontApp.unidentified)
        let u = FrontApp.unidentified
        #expect(FrontAppGuard.refusalNextToGesture(onSettings, front: u)?.failure == .stale)
        #expect(FrontAppGuard.refusalNextToGesture(onLayer, front: settings)?.failure == .stale)
        // The same bundle id both ways: the runner's own check tells the surfaces apart.
        #expect(SystemSurface.staleRefusal(expect: onLayer, top: u, planned: .controlCentre, now: .controlCentre) == nil)
        #expect(SystemSurface.staleRefusal(expect: onLayer, top: u, planned: .controlCentre, now: .notificationCentre)?.failure == .stale)
        #expect(SystemSurface.staleRefusal(expect: onLayer, top: u, planned: nil, now: .appSwitcher)?.failure == .stale)
        #expect(SystemSurface.staleRefusal(expect: onLayer, top: u, planned: .appSwitcher, now: nil)?.failure == .stale)
        #expect(SystemSurface.staleRefusal(expect: onLayer, top: u, planned: nil, now: nil) == nil)
        // Not its business: no expect, or a layer that is an app.
        #expect(SystemSurface.staleRefusal(expect: nil, top: u, planned: nil, now: .controlCentre) == nil)
        #expect(SystemSurface.staleRefusal(expect: onSettings, top: settings, planned: .controlCentre, now: nil) == nil)
        let message = SystemSurface.staleRefusal(expect: onLayer, top: u, planned: .controlCentre, now: .notificationCentre)?.error ?? ""
        #expect(message.contains("Control Centre") && message.contains("Notification Centre is up"))
        #expect(!(SystemSurface.staleRefusal(expect: onLayer, top: u, planned: .controlCentre, now: .notificationCentre)?.ok ?? true))
    }

    // MARK: - The lock interplay (SBCoverSheetWindow is the lock screen's window too)

    /// The readings the runner takes from SpringBoard's tree with the notify state.
    private func signals(_ notify: Bool?, _ root: RawElement, front: LockSignals.Front) -> LockSignals {
        LockSignals(notify: notify, lockScreen: LockScreenSignature.matches(root), front: front,
                    notificationCentre: SystemSurface.up(inSpringBoard: root) == .notificationCentre)
    }

    @Test("Notification Centre on an unlocked phone is unlocked even with the notify state true (run 139); the lock screen is still locked")
    func lockInterplay() throws {
        let nc = try RawTreeText.parse(RealSystemSurfaceTrees.notificationCentre.text)
        let lock = try RawTreeText.parse(RealSpringBoardTrees.lockScreen.text)
        let app = LockSignals.Front.app(settings)
        // What tells them apart: the lock screen's signature only.
        #expect(!LockScreenSignature.matches(nc) && LockScreenSignature.matches(lock))
        #expect(has(nc, id: "SBCoverSheetWindow") && has(lock, id: "SBCoverSheetWindow"))
        // Run 139: notify read true with Notification Centre over Settings.
        let ncReading = signals(true, nc, front: app)
        #expect(ncReading.notificationCentre && ncReading.lockScreen == false)
        #expect(LockJudge().judge(ncReading, at: .zero).state == .unlocked)
        #expect(ncReading.description.contains("Notification Centre up"))
        // Notify false, as a device would say: unlocked too.
        #expect(LockJudge().judge(signals(false, nc, front: app), at: .zero).state == .unlocked)
        // The lock screen, with an app reading as in front (audit C6) or SpringBoard: locked,
        // whatever the notify state says, however long.
        for front in [app, .springBoard] {
            let judge = LockJudge()
            for notify in [true, false, nil] as [Bool?] {
                let reading = signals(notify, lock, front: front)
                #expect(!reading.notificationCentre)
                #expect(judge.judge(reading, at: .seconds(40)).state == .locked, "\(reading)")
            }
        }
        // Without the new reading (notificationCentre false) the same tree would be locked
        // for 12 s: the rule of before, kept for every other tree.
        let before = LockSignals(notify: true, lockScreen: false, front: app)
        #expect(LockJudge().judge(before, at: .zero).state == .locked)
        // The Notification Centre reading never latches the notify state as stale.
        let judge = LockJudge()
        for t in stride(from: 0.0, through: 30.0, by: 1.0) {
            #expect(judge.judge(ncReading, at: .seconds(t)).staleNote == nil)
        }
    }

    @Test("run 139's readings in order: unlocked on the Home Screen, with Notification Centre, Control Centre and the App Switcher")
    func run139Sequence() throws {
        let judge = LockJudge()
        let home = try RawTreeText.parse(RealSystemSurfaceTrees.homeBadge.text)
        let nc = try RawTreeText.parse(RealSystemSurfaceTrees.notificationCentre.text)
        let cc = try RawTreeText.parse(RealSystemSurfaceTrees.controlCentre.text)
        let sw = try RawTreeText.parse(RealSystemSurfaceTrees.appSwitcher.text)
        let app = LockSignals.Front.app(settings)
        // (t, notify as recorded, tree, front)
        let rows: [(Double, Bool, RawElement, LockSignals.Front)] = [
            (24.7, false, home, .springBoard),
            (44.7, true, nc, app),
            (53.8, false, cc, app),
            (65.0, false, sw, app),
        ]
        for (t, notify, root, front) in rows {
            #expect(judge.judge(signals(notify, root, front: front), at: .seconds(t)).state == .unlocked, "t+\(t)s")
        }
    }
}
