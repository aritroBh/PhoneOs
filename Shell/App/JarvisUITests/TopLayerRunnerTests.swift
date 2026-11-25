import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// The top layer on the simulator: `XCUITestPhoneRunner` driven directly (no agent, no
/// model), as `JarvisRunnerLoopbackTests` and the R23 probe do. The runner must report what is
/// really on top of the screen — a SpringBoard alert over the app, Spotlight (its own
/// process), a notification banner over the app — and refuse a touch planned on another
/// layer (docs/ARCHITECTURE.md, *Swift shell*, the top layer).
///
/// Strict where the product must hold (the alert and Spotlight are the top layer, a touch
/// planned on the app is refused while the alert is up, one planned on the alert goes
/// through); tolerant only where iOS may differ (whether a banner can be produced: the
/// notification permission is decided once per simulator). Each test ends on the Home
/// Screen, in portrait, and attaches its notes (`top-layer.<test>.txt`).
final class TopLayerRunnerTests: QATestCase {
    private static let springboardId = TopLayer.springBoard
    private static let settingsId = "com.apple.Preferences"

    private var lines: [String] = []
    private var started = Date()

    private func note(_ text: String) {
        let seconds = String(format: "%5.1f", Date().timeIntervalSince(started))
        lines.append("t+\(seconds) s  \(text)")
    }

    @MainActor
    private func finish(_ name: String, terminating ids: [String]) async {
        if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
        let own = XCUIApplication()
        if own.state != .notRunning { own.terminate() }
        await SystemAppAgentTests.startOnHomeScreen(terminating: ids)
        note("ended on the Home Screen, portrait")
        let attachment = XCTAttachment(string: "TOP LAYER \(name)\n" + lines.joined(separator: "\n") + "\n")
        attachment.name = "top-layer.\(name).txt"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// Opens the Playground's "Fact probes" row, then the probe screen `id`
    /// (`ProbeFixtures.swift`).
    @MainActor
    private func openProbe(_ app: XCUIApplication, _ id: String) async {
        let entry = app.buttons["playground.probes"].firstMatch
        require(entry, "the Playground's Fact probes row", in: app)
        entry.tap()
        let row = app.buttons[id].firstMatch
        require(row, "the \(id) row", timeout: 5, in: app)
        row.tap()
        try? await Task.sleep(for: .seconds(1))
    }

    private static func find(_ root: UiNode, _ test: (UiNode) -> Bool) -> UiNode? {
        var stack = [root]
        while let node = stack.popLast() {
            if test(node) { return node }
            stack.append(contentsOf: node.children)
        }
        return nil
    }

    private static func centre(_ f: Rect) -> (x: Double, y: Double) {
        (f.x + f.width / 2, f.y + f.height / 2)
    }

    // MARK: - 1. The web sign-in consent alert (SpringBoard's) over Jarvis

    @MainActor
    func testA_SignInConsentAlertIsTheTopLayer() async throws {
        started = Date()
        let server = try FormCaptureServer()
        defer { server.stop() }
        let app = launchPlayground(extraArguments: ["-jarvis-probe-url", server.baseURL.absoluteString + "/"])
        await openProbe(app, "probe.sheets")
        let auth = app.buttons["probe.sheets.auth"].firstMatch
        require(auth, "the Start web sign-in button", timeout: 5, in: app)
        // A point inside the app's own screen, for the tap planned on the app.
        let appPoint = (x: Double(auth.frame.midX), y: Double(auth.frame.midY))
        let runner = XCUITestPhoneRunner()
        runner.noteActivated(Self.appBundleId)
        let before = try await runner.frontBundleId()
        XCTAssertEqual(before, Self.appBundleId, "before the alert the app is on top")
        auth.tap()
        note("tapped Start web sign-in")
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let alert = board.alerts.firstMatch
        guard alert.waitForExistence(timeout: 10) else {
            note("no consent alert in SpringBoard's tree within 10 s")
            await finish("sign-in-consent", terminating: [])
            XCTFail("the web sign-in consent alert did not appear in SpringBoard's tree")
            return
        }
        note("the consent alert is up in SpringBoard's tree: \"\(alert.label)\"")

        // The runner reports the alert as the top layer.
        let front = try await runner.frontBundleId()
        note("frontBundleId() with the alert up: \(front ?? "nil")")
        XCTAssertEqual(front, Self.springboardId)
        let observation = try await runner.snapshot(includeScreenshot: false)
        note("snapshot: \(observation.appBundleId); notes: \(observation.notes.joined(separator: " | "))")
        XCTAssertEqual(observation.appBundleId, Self.springboardId)
        let root = try XCTUnwrap(observation.root, "the runner sent no typed tree")
        XCTAssertNotNil(Self.find(root) { $0.role == .alert }, "the snapshot holds no alert")
        XCTAssertNotNil(Self.find(root) { $0.role == .button && $0.label == "Continue" }, "no Continue button in the snapshot")
        let cancel = try XCTUnwrap(Self.find(root) { $0.role == .button && $0.label == "Cancel" }, "no Cancel button in the snapshot")
        XCTAssertTrue(observation.notes.contains { $0.contains("over \(Self.appBundleId)") },
                      "the observation does not say which app the alert is over: \(observation.notes)")

        // A tap planned on the app is refused as stale, and the alert stays up.
        let stale = try await runner.perform(.tap(x: appPoint.x, y: appPoint.y), expect: CommandExpect(bundleId: Self.appBundleId))
        note("tap planned on the app: ok \(stale.ok), failure \(String(describing: stale.failure)), \(stale.error ?? "")")
        XCTAssertFalse(stale.ok)
        XCTAssertEqual(stale.failure, RunnerFailure.stale)
        XCTAssertTrue(alert.exists, "the alert went away: the refused tap reached the screen")

        // A tap planned on the alert goes through.
        let point = Self.centre(cancel.frame)
        let tapped = try await runner.perform(.tap(x: point.x, y: point.y), expect: CommandExpect(bundleId: Self.springboardId))
        note("tap on Cancel planned on SpringBoard: ok \(tapped.ok) \(tapped.error ?? "")")
        XCTAssertTrue(tapped.ok, tapped.error ?? "")
        let gone = Date().addingTimeInterval(10)
        while alert.exists, Date() < gone { try await Task.sleep(for: .milliseconds(300)) }
        XCTAssertFalse(alert.exists, "the alert is still up after Cancel")

        // The app is on top again.
        var after = try await runner.frontBundleId()
        let back = Date().addingTimeInterval(5)
        while after != Self.appBundleId, Date() < back {
            try await Task.sleep(for: .milliseconds(300))
            after = try await runner.frontBundleId()
        }
        note("frontBundleId() after Cancel: \(after ?? "nil")")
        XCTAssertEqual(after, Self.appBundleId)
        await finish("sign-in-consent", terminating: [])
    }

    // MARK: - 2. Spotlight (its own process)

    @MainActor
    func testB_SpotlightIsTheTopLayer() async throws {
        started = Date()
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        let runner = XCUITestPhoneRunner()
        let home = try await runner.frontBundleId()
        note("frontBundleId() on the Home Screen: \(home ?? "nil")")
        XCTAssertEqual(home, Self.springboardId)
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let start = board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4))
        let end = board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75))
        start.press(forDuration: 0.05, thenDragTo: end)
        note("swiped down on the Home Screen")
        let spotlight = XCUIApplication(bundleIdentifier: TopLayer.spotlight)
        let field = spotlight.textFields[TopLayer.spotlightFieldIdentifier].firstMatch
        guard field.waitForExistence(timeout: 8) else {
            note("Spotlight's field did not appear (Spotlight state \(spotlight.state.rawValue))")
            await finish("spotlight", terminating: [])
            XCTFail("Spotlight's search field did not appear after the swipe down")
            return
        }
        let front = try await runner.frontBundleId()
        note("frontBundleId() with Spotlight up: \(front ?? "nil") (Spotlight state \(spotlight.state.rawValue))")
        XCTAssertEqual(front, TopLayer.spotlight)
        let observation = try await runner.snapshot(includeScreenshot: false)
        note("snapshot: \(observation.appBundleId); notes: \(observation.notes.joined(separator: " | "))")
        XCTAssertEqual(observation.appBundleId, TopLayer.spotlight)
        let root = try XCTUnwrap(observation.root)
        XCTAssertNotNil(Self.find(root) { $0.identifier == TopLayer.spotlightFieldIdentifier }, "the snapshot holds no search field")
        // A touch planned on the Home Screen is refused while Spotlight is on top.
        let stale = try await runner.perform(.tap(x: 220, y: 300), expect: CommandExpect(bundleId: Self.springboardId))
        note("tap planned on SpringBoard: ok \(stale.ok), failure \(String(describing: stale.failure))")
        XCTAssertEqual(stale.failure, RunnerFailure.stale)

        XCUIDevice.shared.press(.home)
        note("pressed Home")
        var after = try await runner.frontBundleId()
        let deadline = Date().addingTimeInterval(8)
        while after != Self.springboardId, Date() < deadline {
            try await Task.sleep(for: .milliseconds(300))
            after = try await runner.frontBundleId()
        }
        note("frontBundleId() after Home: \(after ?? "nil") (Spotlight state \(spotlight.state.rawValue))")
        XCTAssertEqual(after, Self.springboardId, "Spotlight is still reported on top after Home")
        await finish("spotlight", terminating: [])
    }

    // MARK: - 3. A notification banner over an app (tolerant: needs notification permission)

    @MainActor
    func testC_BannerCoversWhatIsUnderIt() async throws {
        started = Date()
        let app = launchPlayground()
        await openProbe(app, "probe.notify")
        let schedule = app.buttons["probe.notify.schedule"].firstMatch
        require(schedule, "the Notify in 6 s button", timeout: 5, in: app)
        schedule.tap()
        note("tapped Notify in 6 s")
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        // The permission alert (first run on a simulator only) is SpringBoard's.
        if board.alerts.firstMatch.waitForExistence(timeout: 4) {
            let runner = XCUITestPhoneRunner()
            runner.noteActivated(Self.appBundleId)
            let front = try await runner.frontBundleId()
            note("the notification permission alert is up; frontBundleId() \(front ?? "nil")")
            XCTAssertEqual(front, Self.springboardId, "the permission alert is not the top layer")
            let allow = board.alerts.firstMatch.buttons["Allow"].firstMatch
            if allow.exists { allow.tap() }
            note(allow.exists ? "tapped Allow" : "the alert has no Allow button")
        }
        let status = app.staticTexts["probe.notify.status"].firstMatch
        var statusText = ""
        let statusDeadline = Date().addingTimeInterval(8)
        while Date() < statusDeadline {
            statusText = status.exists ? status.label : ""
            if statusText == "Scheduled" || statusText == "Not allowed" { break }
            try await Task.sleep(for: .milliseconds(300))
        }
        note("notify status: \"\(statusText)\"")
        guard statusText == "Scheduled" else {
            await finish("banner", terminating: [Self.settingsId])
            throw XCTSkip("no notification could be scheduled (status \"\(statusText)\"): no banner to check")
        }
        let settings = XCUIApplication(bundleIdentifier: Self.settingsId)
        settings.activate()
        _ = settings.wait(for: .runningForeground, timeout: 10)
        note("Settings in front")
        let bannerQuery = board.descendants(matching: .any).matching(identifier: BannerCover.identifier).firstMatch
        guard bannerQuery.waitForExistence(timeout: 15) else {
            await finish("banner", terminating: [Self.settingsId])
            throw XCTSkip("no banner appeared in SpringBoard's tree within 15 s")
        }
        let queried = bannerQuery.frame
        note("banner by query: frame {\(queried.minX), \(queried.minY), \(queried.width)x\(queried.height)}")
        let runner = XCUITestPhoneRunner()
        let observation = try await runner.snapshot(includeScreenshot: false)
        note("snapshot of \(observation.appBundleId); notes: \(observation.notes.joined(separator: " | "))")
        guard let root = observation.root,
              let cover = Self.find(root, { $0.role == .button && $0.label == nil && !$0.enabled && !$0.hittable && $0.frame.width > 100 })
        else {
            let stillUp = bannerQuery.exists
            await finish("banner", terminating: [Self.settingsId])
            if stillUp { XCTFail("a banner is up but the runner's tree holds no covering node") }
            throw XCTSkip("the banner went away before the runner's snapshot")
        }
        note("covering node: {\(cover.frame.x), \(cover.frame.y), \(cover.frame.width)x\(cover.frame.height)}")
        XCTAssertEqual(observation.appBundleId, Self.settingsId)
        XCTAssertLessThanOrEqual(cover.frame.width, observation.screenSize.width, "the covering node is in pixels, not points")
        XCTAssertEqual(cover.frame.x, Double(queried.minX), accuracy: 6)
        XCTAssertEqual(cover.frame.y, Double(queried.minY), accuracy: 6)
        XCTAssertEqual(cover.frame.width, Double(queried.width), accuracy: 6)
        XCTAssertEqual(cover.frame.height, Double(queried.height), accuracy: 6)
        // A tap at its centre planned without it (a runner that reported no banner) is refused.
        let fresh = XCUITestPhoneRunner()
        let point = Self.centre(cover.frame)
        if bannerQuery.exists {
            let result = try await fresh.perform(.tap(x: point.x, y: point.y), expect: CommandExpect(bundleId: Self.settingsId))
            note("tap at the banner's centre planned without it: ok \(result.ok), \(result.error ?? "")")
            if result.ok {
                XCTAssertFalse(bannerQuery.exists, "a tap in a banner the look did not hold was carried out while the banner was up")
                note("the banner was gone by the touch; the tap was carried out")
            } else {
                XCTAssertEqual(result.failure, RunnerFailure.stale)
            }
        } else {
            note("the banner went away before the touch check; not checked")
        }
        let goneDeadline = Date().addingTimeInterval(10)
        while bannerQuery.exists, Date() < goneDeadline { try await Task.sleep(for: .milliseconds(500)) }
        await finish("banner", terminating: [Self.settingsId])
    }
}
