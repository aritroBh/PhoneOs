import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// The front-app rule (`FrontApp`, `TopLayer.decide`): the pure decisions the Mac-only
/// XCUITest runner makes, on the real SpringBoard trees of the iOS 26.5 simulator runs 134,
/// 135 and 137 (`RealSpringBoardTrees`).
@Suite("Front app: an app Jarvis cannot identify is never the Home Screen (FrontApp)")
struct FrontAppTests {
    private func tree(_ c: RealSpringBoardTrees.Capture) throws -> RawElement {
        try RawTreeText.parse(c.text)
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

    @Test("the parser reads every capture whole: SpringBoard's application element, every line a node")
    func parser() throws {
        for c in RealSpringBoardTrees.all {
            let root = try tree(c)
            #expect(root.elementType == XCUIElementTypeCode.application, "\(c.name)")
            #expect(root.count == c.text.split(separator: "\n").count, "\(c.name)")
            // Every identifier and label of the text is read (the frames' last digits may print
            // differently: the dump rounds).
            let ids = c.text.components(separatedBy: " id=").count - 1
            let labels = c.text.components(separatedBy: " label=\"").count - 1
            #expect(count(root) { !$0.identifier.isEmpty } == ids, "\(c.name)")
            #expect(count(root) { !$0.label.isEmpty } == labels, "\(c.name)")
            let dumped = UiTreeBuilder.dump(root).split(separator: "\n")
            let original = c.text.split(separator: "\n")
            #expect(dumped.count == original.count, "\(c.name)")
            for (a, b) in zip(dumped, original) {
                // The same type, label and identifier on each line.
                #expect(a.prefix { $0 != "{" } == b.prefix { $0 != "{" }, "\(c.name): \(a) / \(b)")
            }
        }
    }

    @Test("the distinguishing fact: the Home Screen's identifier is in every Home Screen capture and in no capture with an app in front, an alert or the lock screen")
    func distinguishingFact() throws {
        var seen: [RealSpringBoardTrees.Kind: Int] = [:]
        for c in RealSpringBoardTrees.all {
            let root = try tree(c)
            let showing = FrontApp.homeScreenShowing(root)
            seen[c.kind, default: 0] += 1
            switch c.kind {
            case .home, .spotlight:
                #expect(showing, "\(c.name): \(c.showing) (\(c.source))")
            case .app, .alert, .lock:
                #expect(!showing, "\(c.name): \(c.showing) (\(c.source))")
            }
        }
        // The corpus: every kind is there, and the app-in-front captures span two apps and
        // six situations.
        #expect(seen[.home] == 5 && seen[.spotlight] == 2 && seen[.app] == 11 && seen[.alert] == 2 && seen[.lock] == 1)
    }

    @Test("the evidence, node by node: which identifier says Home Screen in each Home capture, and what an app-in-front capture holds instead")
    func evidence() throws {
        for c in RealSpringBoardTrees.all {
            let root = try tree(c)
            let icons = count(root) { $0.identifier == FrontApp.homeScreenIdentifier }
            let menu = count(root) { $0.identifier.hasPrefix(FrontApp.homeScreenIdentifierPrefix) }
            let switcherCard = count(root) { $0.identifier.hasPrefix("card:") }
            switch c.kind {
            case .home where c.name == "homeIconMenu":
                // The icon menu: the menu's window only, its shortcut items in SpringBoard
                // Home's namespace.
                #expect(icons == 0 && menu == 2, "\(c.name)")
            case .home, .spotlight:
                #expect(icons == 1 && menu == 0, "\(c.name)")
                #expect(switcherCard == 0, "\(c.name)")
            case .app:
                // SpringBoard's tree with an app in front holds the app switcher's card of
                // that app, and no icons.
                #expect(icons == 0 && menu == 0, "\(c.name)")
                #expect(switcherCard >= 1, "\(c.name)")
            case .alert, .lock:
                #expect(icons == 0 && menu == 0 && switcherCard == 0, "\(c.name)")
            }
        }
        // The card names the app in front (Jarvis, or Settings under the banner).
        let settings = try tree(RealSpringBoardTrees.settingsBanner134)
        #expect(count(settings) { $0.identifier.hasPrefix("card:com.apple.Preferences:") } == 1)
    }

    @Test("the lock screen capture is the lock screen (LockScreenSignature) and nothing else is")
    func lockScreen() throws {
        for c in RealSpringBoardTrees.all {
            #expect(LockScreenSignature.matches(try tree(c)) == (c.kind == .lock), "\(c.name)")
        }
    }

    /// The runner's whole decision on each real tree, with what its queries would have read
    /// (an alert in SpringBoard's tree; Spotlight in front for the Spotlight captures; the
    /// lock verdict for the lock screen): only a Home Screen (or the lock screen) is
    /// SpringBoard, an app in front is the unidentified app, never the Home Screen.
    @Test("the top layer on every real tree, with SpringBoard as the foreground reading (the old twelve-app list missed the app)")
    func topLayerOnRealTrees() throws {
        for c in RealSpringBoardTrees.all {
            let root = try tree(c)
            let alertUp = count(root) { $0.elementType == XCUIElementTypeCode.alert } > 0
            let spotlightUp = c.kind == .spotlight
            let locked = LockScreenSignature.matches(root)
            let shows: TopLayer.SpringBoardShows = FrontApp.homeScreenShowing(root) ? .homeScreen : (locked ? .lockScreen : .other)
            let top = TopLayer.decide(systemAlertUp: alertUp, spotlightInFront: spotlightUp,
                                      foreground: TopLayer.springBoard, springBoard: shows)
            switch c.kind {
            case .home, .lock, .alert:
                #expect(top.bundleId == TopLayer.springBoard, "\(c.name)")
            case .spotlight:
                #expect(top.bundleId == TopLayer.spotlight, "\(c.name)")
            case .app:
                #expect(top.bundleId == FrontApp.unidentified, "\(c.name): \(c.showing)")
                #expect(top.note == FrontApp.unidentifiedNote, "\(c.name)")
            }
            #expect(alertUp == (c.kind == .alert), "\(c.name)")
        }
    }

    @Test("an app that is identified is its own top layer; the alert and Spotlight still come first")
    func identified() {
        let mail = "com.apple.mobilemail"
        #expect(TopLayer.decide(systemAlertUp: false, spotlightInFront: false, foreground: mail, springBoard: .other)
            == TopLayer.Decision(bundleId: mail))
        #expect(TopLayer.decide(systemAlertUp: true, spotlightInFront: false, foreground: mail, springBoard: .other)
            == TopLayer.Decision(bundleId: TopLayer.springBoard, alertOver: mail, systemAlert: true))
        #expect(TopLayer.decide(systemAlertUp: false, spotlightInFront: true, foreground: mail, springBoard: .other)
            == TopLayer.Decision(bundleId: TopLayer.spotlight))
        // The SpringBoard reading is never passed through without its Home Screen: the
        // default a runner gives when it did not read it (`.other`) fails closed.
        #expect(TopLayer.decide(systemAlertUp: false, spotlightInFront: false, foreground: " COM.APPLE.springboard ", springBoard: .other)
            .bundleId == FrontApp.unidentified)
    }

    @Test("touch time: a command planned on SpringBoard is stale once the unidentified app is on top, and the other way round")
    func stale() {
        let onHome = CommandExpect(bundleId: TopLayer.springBoard)
        let onUnknown = CommandExpect(bundleId: FrontApp.unidentified)
        #expect(FrontAppGuard.refusalNextToGesture(onHome, front: FrontApp.unidentified)?.failure == .stale)
        #expect(FrontAppGuard.refusalNextToGesture(onUnknown, front: TopLayer.springBoard)?.failure == .stale)
        #expect(FrontAppGuard.refusalNextToGesture(onUnknown, front: FrontApp.unidentified) == nil)
        #expect(FrontAppGuard.refusalNextToGesture(onHome, front: TopLayer.springBoard) == nil)
    }

    @Test("the observation of the unidentified app: the application node only, no label, value or identifier; never SpringBoard's tree")
    func emptyTree() {
        let screen = Rect(x: 0, y: 0, width: 440, height: 956)
        let raw = FrontApp.emptyRaw(screen: screen)
        #expect(raw.children.isEmpty && raw.label.isEmpty && raw.identifier.isEmpty && raw.value == nil)
        let built = UiTreeBuilder.build(raw, screenSize: Size(width: 440, height: 956))
        #expect(built.root.role == .application)
        #expect(built.root.children.isEmpty)
        #expect(built.root.label == nil && built.root.value == nil && built.root.identifier == nil)
        #expect(built.root.frame == screen)
        #expect(!built.keyboardVisible)
        #expect(FrontApp.unidentified == "unknown.front.app", "mirrored in the core: jarvis_proto::UNIDENTIFIED_APP_BUNDLE_ID")
        #expect(FrontApp.isUnidentified(" UNKNOWN.front.app "))
        #expect(!TopLayer.isSpringBoard(FrontApp.unidentified))
        #expect(FrontApp.unidentifiedNote.contains("cannot identify"))
    }

    // MARK: - Part 2: choosing among the accessibility client's active applications

    @Test("choose: one app over SpringBoard is it; SpringBoard alone is SpringBoard; the runner and Spotlight are never the front app")
    func chooseTable() {
        let sb = TopLayer.springBoard
        let runner = "dev.jarvis.JarvisUITests.xctrunner"
        let never: (String) -> Bool = { _ in Issue.record("no foreground check needed"); return false }
        #expect(FrontApp.choose(active: ["com.apple.mobilemail"], runner: runner, isForeground: never).bundleId == "com.apple.mobilemail")
        #expect(FrontApp.choose(active: [sb, "com.apple.mobilemail"], runner: runner, isForeground: never).bundleId == "com.apple.mobilemail")
        #expect(FrontApp.choose(active: [sb], runner: runner, isForeground: never).bundleId == sb)
        #expect(FrontApp.choose(active: [sb, runner], runner: runner, isForeground: never).bundleId == sb)
        #expect(FrontApp.choose(active: [sb, TopLayer.spotlight], runner: runner, isForeground: never).bundleId == sb)
        #expect(FrontApp.choose(active: [sb, sb, " com.apple.Passbook "], runner: runner, isForeground: never).bundleId == "com.apple.Passbook")
        // The synthetic id is no app.
        #expect(FrontApp.choose(active: [sb, FrontApp.unidentified], runner: runner, isForeground: never).bundleId == sb)
    }

    @Test("choose: what does not tell is nil (the public readings and the fail-closed rule decide)")
    func chooseUndecided() {
        let sb = TopLayer.springBoard
        let never: (String) -> Bool = { _ in false }
        // Nothing active.
        #expect(FrontApp.choose(active: [], runner: nil, isForeground: never).bundleId == nil)
        // An active process whose bundle id was not read may be the front app.
        #expect(FrontApp.choose(active: [sb, nil], runner: nil, isForeground: never).bundleId == nil)
        #expect(FrontApp.choose(active: ["com.apple.mobilemail", ""], runner: nil, isForeground: never).bundleId == nil)
        // Neither SpringBoard nor an app (only the runner itself).
        #expect(FrontApp.choose(active: ["x.runner"], runner: "x.runner", isForeground: never).bundleId == nil)
        // Two apps: the one in the foreground, else not decided.
        let two: [String?] = [sb, "com.apple.mobilemail", "com.apple.InCallService"]
        #expect(FrontApp.choose(active: two, runner: nil) { $0 == "com.apple.InCallService" }.bundleId == "com.apple.InCallService")
        #expect(FrontApp.choose(active: two, runner: nil, isForeground: never).bundleId == nil)
        #expect(FrontApp.choose(active: two, runner: nil) { _ in true }.bundleId == nil)
        // The reason is kept for the notes.
        #expect(FrontApp.choose(active: two, runner: nil, isForeground: never).why.contains("not decided"))
    }
}
