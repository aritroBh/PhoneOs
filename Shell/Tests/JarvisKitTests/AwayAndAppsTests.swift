import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

@MainActor
private final class FakeDisplay: AwayDisplay {
    var brightness: Double
    var isIdleTimerDisabled = false
    var isProximityMonitoringEnabled = false

    init(brightness: Double) {
        self.brightness = brightness
    }
}

@Suite("Away mode")
@MainActor
struct AwayModeTests {
    @Test("first use explains; acknowledging dims; exit restores everything")
    func firstUse() {
        let display = FakeDisplay(brightness: 0.7)
        display.isIdleTimerDisabled = false
        let away = AwayModeController(display: display, acknowledged: false)
        #expect(away.request() == .explaining)
        #expect(display.brightness == 0.7, "dimmed before the owner accepted the costs")
        away.dismissExplanation()
        #expect(away.phase == .off)
        #expect(!away.acknowledged)

        #expect(away.request() == .explaining)
        away.acknowledge()
        #expect(away.acknowledged)
        #expect(away.phase == .on)
        #expect(display.brightness == 0)
        #expect(display.isIdleTimerDisabled)
        #expect(!display.isProximityMonitoringEnabled, "proximity is off by default")
        #expect(away.savedBrightness == 0.7)

        away.exit()
        #expect(away.phase == .off)
        #expect(display.brightness == 0.7)
        #expect(!display.isIdleTimerDisabled)
        // A second exit must not write anything (in particular not 0).
        display.brightness = 0.4
        away.exit()
        #expect(display.brightness == 0.4)
    }

    @Test("acknowledged: straight in; background and terminate restore; foreground dims again")
    func lifecycle() {
        let display = FakeDisplay(brightness: 0.5)
        display.isIdleTimerDisabled = true // the owner's own setting is put back, not forced off
        let away = AwayModeController(display: display, acknowledged: true, proximity: true)
        #expect(away.request() == .on)
        #expect(display.brightness == 0)
        #expect(display.isProximityMonitoringEnabled)

        away.appLeftForeground()
        #expect(display.brightness == 0.5)
        #expect(display.isIdleTimerDisabled)
        #expect(!display.isProximityMonitoringEnabled)
        #expect(away.phase == .on, "Away mode stays chosen while in the background")
        away.appLeftForeground()
        #expect(display.brightness == 0.5, "leaving the foreground twice wrote over the brightness")

        display.brightness = 0.6 // the owner changed it in Control Centre meanwhile
        away.appReturnedToForeground()
        #expect(display.brightness == 0)
        #expect(away.savedBrightness == 0.6)

        away.setProximity(false)
        #expect(!display.isProximityMonitoringEnabled)

        away.appWillTerminate()
        #expect(display.brightness == 0.6)
        #expect(away.phase == .off)
        away.exit()
        #expect(display.brightness == 0.6)
    }

    @Test("entering while in the background waits for the foreground")
    func enterInBackground() {
        let display = FakeDisplay(brightness: 0.3)
        let away = AwayModeController(display: display, acknowledged: true)
        away.appLeftForeground()
        away.request()
        #expect(away.phase == .on)
        #expect(display.brightness == 0.3)
        away.appReturnedToForeground()
        #expect(display.brightness == 0)
        away.exit()
        #expect(display.brightness == 0.3)
    }

    @Test("the copy states every cost from the plan")
    func copy() {
        let text = AwayModeCopy.costs.map { $0.title + " " + $0.detail }.joined(separator: " ").lowercased()
        for needle in ["unlocked", "pocket", "charger", "home or desk"] {
            #expect(text.contains(needle), "missing: \(needle)")
        }
    }

    @Test("settings remember the acknowledgement and default it off")
    func settings() throws {
        #expect(!JarvisSettings().awayAcknowledged)
        #expect(!JarvisSettings().awayProximity)
        var s = JarvisSettings()
        s.awayAcknowledged = true
        let decoded = try JSONDecoder().decode(JarvisSettings.self, from: JSONEncoder().encode(s))
        #expect(decoded.awayAcknowledged)
        let old = try JSONDecoder().decode(JarvisSettings.self, from: Data(#"{"policy_mode":"guarded"}"#.utf8))
        #expect(!old.awayAcknowledged)
    }
}

@Suite("Apps: ways in")
struct AppAccessTests {
    @Test("screen only unless a tool for the app is registered; denied closes every way")
    func derive() throws {
        var settings = JarvisSettings()
        settings.setTrustForTest("com.apple.Preferences", .guarded)
        let tools = [
            ToolSpecs.spec(name: "clock_now", description: "time"),
            ToolSpecs.spec(name: "calendar_add", description: "add", tier: .native, app: "com.apple.mobilecal"),
            ToolSpecs.spec(name: "notes_shortcut", description: "note", tier: .shortcut, app: "com.apple.mobilenotes"),
            ToolSpecs.spec(name: "shop_order", description: "order", tier: .shortcut, app: "com.example.shop"),
        ]
        let apps = AppAccessCatalog.make(settings: settings, knownApps: KnownApps.common, tools: tools,
                                         coreDeniedApps: try defaultPolicy().deniedApps)
        let byId = Dictionary(uniqueKeysWithValues: apps.map { ($0.bundleId, $0) })

        #expect(Array(apps.prefix(settings.appTrust.count)).map(\.bundleId) == settings.appTrust.map(\.bundleIdPrefix),
                "the owner's list comes first, in order")
        let settingsApp = try #require(byId["com.apple.Preferences"])
        #expect(settingsApp.has(.screen))
        #expect(!settingsApp.has(.appActions))
        #expect(!settingsApp.has(.direct))
        #expect(settingsApp.trust == .guarded)
        #expect(settingsApp.waysDescription == "Ways in: screen")

        #expect(byId["com.apple.mobilecal"]?.directTools == ["calendar_add"])
        #expect(byId["com.apple.mobilecal"]?.has(.direct) == true)
        #expect(byId["com.apple.mobilecal"]?.trust == nil)
        #expect(byId["com.apple.mobilenotes"]?.appActions == ["notes_shortcut"])
        #expect(byId["com.apple.mobilenotes"]?.has(.appActions) == true)
        #expect(byId["com.example.shop"]?.name == "com.example.shop", "an app known only from a tool is listed by id")
        #expect(!apps.contains { $0.appActions.contains("clock_now") || $0.directTools.contains("clock_now") })

        let wallet = try #require(byId["com.apple.Passbook"])
        #expect(wallet.trust == .denied)
        #expect(!wallet.has(.screen))
        #expect(wallet.waysDescription == "No way in")

        // The core's own deny list closes the screen even without an owner rule.
        let passwords = try #require(byId["com.apple.Passwords"])
        let coreDenied = try defaultPolicy().deniedApps
        #expect(passwords.deniedByCore == coreDenied.contains { "com.apple.Passwords" == $0 || "com.apple.Passwords".hasPrefix($0 + ".") })
    }

    @Test("the demo backend lists the demo app with the screen only")
    func demo() async {
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: FileManager.default.temporaryDirectory), selection: .demo)
        let apps = AppAccessCatalog.make(settings: JarvisSettings(), knownApps: coordinator.knownApps, tools: await coordinator.toolSpecs())
        let demo = apps.first { $0.bundleId == DemoPhone.bundleId }
        #expect(demo?.has(.screen) == true)
        #expect(demo?.has(.appActions) == false)
        #expect(demo?.has(.direct) == false)
    }
}

private extension JarvisSettings {
    mutating func setTrustForTest(_ id: String, _ level: TrustLevelOption) {
        if let i = appTrust.firstIndex(where: { $0.bundleIdPrefix == id }) { appTrust[i].level = level }
    }
}
