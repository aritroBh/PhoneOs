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

    /// Every node of `root` that passes `test`, in document order.
    private static func findAll(_ root: UiNode, _ test: (UiNode) -> Bool) -> [UiNode] {
        var out: [UiNode] = []
        var stack = [root]
        while let node = stack.popLast() {
            if test(node) { out.append(node) }
            stack.append(contentsOf: node.children.reversed())
        }
        return out
    }

    /// The shape of the runner's banner cover (`BannerCover.node`): a button with no label,
    /// disabled.
    private static func isCover(_ node: UiNode) -> Bool {
        node.role == .button && (node.label ?? "").isEmpty && !node.enabled
    }

    /// A node with a place of its own on the screen (not the app, a window, a scroll
    /// container or an untyped wrapper).
    private static func isPlaced(_ node: UiNode) -> Bool {
        switch node.role {
        case .application, .window, .other, .collectionView, .scrollView, .table: return false
        default: return true
        }
    }

    /// A node a person taps.
    private static func isControl(_ node: UiNode) -> Bool {
        switch node.role {
        case .button, .link, .cell, .switch, .slider, .textField, .searchField, .tab: return true
        default: return false
        }
    }

    private static func intersection(_ a: Rect, _ b: Rect) -> Rect? {
        let x0 = max(a.x, b.x)
        let y0 = max(a.y, b.y)
        let x1 = min(a.x + a.width, b.x + b.width)
        let y1 = min(a.y + a.height, b.y + b.height)
        guard x1 > x0, y1 > y0 else { return nil }
        return Rect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }

    /// Every banner node in SpringBoard's raw tree, with the window that holds it.
    private static func rawBanners(_ root: RawElement) -> [(banner: RawElement, window: RawElement?)] {
        var out: [(banner: RawElement, window: RawElement?)] = []
        var stack: [(RawElement, RawElement?)] = [(root, nil)]
        while let (element, window) = stack.popLast() {
            let holder = element.elementType == XCUIElementTypeCode.window ? element : window
            if element.identifier == BannerCover.identifier {
                out.append((banner: element, window: holder))
                continue
            }
            for child in element.children.reversed() { stack.append((child, holder)) }
        }
        return out
    }

    private static func rectText(_ f: Rect) -> String {
        String(format: "{%.1f, %.1f, %.1fx%.1f}", f.x, f.y, f.width, f.height)
    }

    private static func cgText(_ r: CGRect) -> String {
        String(format: "{%.1f, %.1f, %.1fx%.1f}", Double(r.minX), Double(r.minY), Double(r.width), Double(r.height))
    }

    private static func describe(_ n: UiNode) -> String {
        "role \(n.role), label \"\(n.label ?? "")\", value \"\(n.value ?? "")\", placeholder \"\(n.placeholder ?? "")\", "
            + "id \"\(n.identifier ?? "")\", frame \(rectText(n.frame)), enabled \(n.enabled), hittable \(n.hittable)"
    }

    private static func seconds(_ t: TimeInterval) -> String {
        String(format: "%.1f", t)
    }

    /// The frame XCUITest's query reports for `element`, nil when it does not exist now.
    @MainActor
    private func queryFrame(_ element: XCUIElement) -> CGRect? {
        element.exists ? element.frame : nil
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
        note("alert by query: frame \(Self.cgText(alert.frame))")
        if let alertNode = Self.find(root, { $0.role == .alert }) {
            note("alert node in the snapshot: \(Self.describe(alertNode))")
            let buttons = Self.findAll(alertNode) { $0.role == .button }
            note("the alert node holds \(buttons.count) button(s)")
            for button in buttons { note("  alert button: \(Self.describe(button))") }
        } else {
            note("no alert node in the snapshot")
        }
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
        let fieldNode = Self.find(root) { $0.identifier == TopLayer.spotlightFieldIdentifier }
        note(fieldNode.map { "search field node in the snapshot: \(Self.describe($0))" } ?? "no search field node in the snapshot")
        XCTAssertNotNil(fieldNode, "the snapshot holds no search field")
        // A touch planned on the Home Screen is refused while Spotlight is on top.
        let stale = try await runner.perform(.tap(x: 220, y: 300), expect: CommandExpect(bundleId: Self.springboardId))
        note("tap planned on SpringBoard: ok \(stale.ok), failure \(String(describing: stale.failure))")
        XCTAssertEqual(stale.failure, RunnerFailure.stale)

        XCUIDevice.shared.press(.home)
        note("pressed Home")
        // Record only (no assert): how the top-layer reading moves once Spotlight is
        // dismissed (run 137: Spotlight still read as on top for about 2 s).
        var readings: [String] = []
        let pressed = Date()
        while Date().timeIntervalSince(pressed) < 5 {
            let at = Date().timeIntervalSince(pressed)
            let id = try await runner.frontBundleId()
            readings.append(String(format: "%.2f s ", at) + (id ?? "nil"))
            try await Task.sleep(for: .milliseconds(250))
        }
        note("frontBundleId() every 250 ms for 5 s after Home (seconds after the press): " + readings.joined(separator: ", "))
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

    /// Seconds from the tap on Notify to the notification (`-jarvis-probe-notify-delay`):
    /// long enough for Settings to reach the front first (run 137: 13 s from `activate()` to
    /// the foreground on Settings' first launch, so a 6 s notification fired over Jarvis).
    private static let notifyDelay = 25.0
    /// Least time left before the notification is due once Settings is in front; with less
    /// the race is lost (the banner may come before Settings is really on top).
    private static let leastMargin = 5.0

    /// When the probe notification is due: the app's own reading (`probe.notify.due`, the
    /// clock the simulator shares with the test runner), else the window from the last action
    /// before scheduling (`earliest`) to the moment "Scheduled" was read (`latest`), plus the
    /// delay.
    @MainActor
    private func dueWindow(_ app: XCUIApplication, earliest: Date, latest: Date) -> (earliest: Date, latest: Date) {
        let field = app.staticTexts["probe.notify.due"].firstMatch
        let text = field.exists ? field.label : ""
        if let epoch = Double(text), epoch.isFinite {
            let at = Date(timeIntervalSince1970: epoch)
            note("the app reports the notification due at \(text), \(Self.seconds(at.timeIntervalSinceNow)) s from now")
            return (at, at)
        }
        let window = (earliest: earliest.addingTimeInterval(Self.notifyDelay), latest: latest.addingTimeInterval(Self.notifyDelay))
        note("the app's due time is unreadable (\"\(text)\"); due between \(Self.seconds(window.earliest.timeIntervalSinceNow)) and \(Self.seconds(window.latest.timeIntervalSinceNow)) s from now")
        return window
    }

    /// Waits until a notification due at `due` has come and gone (bounded: about 25 s past
    /// `due` at most), so its banner does not cover the next test's first screen.
    @MainActor
    private func waitOutBanner(due: Date, _ banner: XCUIElement) async {
        let ahead = due.timeIntervalSinceNow + 2
        if ahead > 0 { try? await Task.sleep(for: .milliseconds(Int(min(ahead, 30) * 1_000))) }
        let deadline = Date().addingTimeInterval(10)
        while banner.exists, Date() < deadline { try? await Task.sleep(for: .milliseconds(500)) }
    }

    /// The banner over Settings: the runner's look of Settings holds exactly one covering
    /// node at the banner's frame; a touch in the banner, or on a Settings control under it,
    /// planned with a look taken before the banner is refused and nothing is synthesized; a
    /// control well below is still tapped; once the banner has gone a fresh look has no cover.
    ///
    /// Timing (typical, run 137's steps): launch and the probe screen ~19 s, permission alert
    /// and "Scheduled" ~23 s, notification due ~23 + 25 = ~48 s; Settings in front by ~36 s
    /// (13 s on a first launch) leaves ~12 s (race lost below 5 s); the banner is polled from
    /// then until 15 s after it is due (~63 s), the checks take ~5 s, the banner goes within
    /// ~8 s, the end ~10 s: ~70-80 s. Worst case (every wait to its limit): launch 20 + probe
    /// screen 16 + alert 4 + status 8 + due within 25 s of the status, polled to 15 s after it
    /// (40, Settings' 15 s wait inside it) + checks 10 + banner gone 12 + end 12 = ~122 s,
    /// inside the default 180 s allowance (`sim-qa.sh`).
    @MainActor
    func testC_BannerCoversWhatIsUnderIt() async throws {
        // Every observed value reaches the attachment, also after a failed assertion.
        continueAfterFailure = true
        started = Date()
        var unchecked: [String] = []
        let app = launchPlayground(extraArguments: ["-jarvis-probe-notify-delay", String(format: "%g", Self.notifyDelay)])
        await openProbe(app, "probe.notify")
        let schedule = app.buttons["probe.notify.schedule"].firstMatch
        require(schedule, "the Notify button", timeout: 5, in: app)
        note("the Notify button: \"\(schedule.label)\"")
        var scheduledAfter = Date()
        schedule.tap()
        note("tapped Notify (delay \(Self.notifyDelay.compactText) s)")
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        // The permission alert (first run on a simulator only) is SpringBoard's.
        let permission = board.alerts.firstMatch
        if permission.waitForExistence(timeout: 4) {
            let runner = XCUITestPhoneRunner()
            runner.noteActivated(Self.appBundleId)
            let front = try await runner.frontBundleId()
            note("the notification permission alert appeared: \"\(permission.label)\"; frontBundleId() \(front ?? "nil")")
            XCTAssertEqual(front, Self.springboardId, "the permission alert is not the top layer")
            let allow = permission.buttons["Allow"].firstMatch
            let hasAllow = allow.exists
            if hasAllow {
                scheduledAfter = Date()
                allow.tap()
            }
            note(hasAllow ? "the alert has an Allow button: tapped Allow" : "the alert has no Allow button; nothing tapped")
        } else {
            note("no notification permission alert appeared within 4 s (permission already decided on this simulator)")
        }
        let status = app.staticTexts["probe.notify.status"].firstMatch
        var statusText = ""
        let statusDeadline = Date().addingTimeInterval(8)
        while Date() < statusDeadline {
            statusText = status.exists ? status.label : ""
            if statusText.hasPrefix("Scheduled") || statusText == "Not allowed" { break }
            try await Task.sleep(for: .milliseconds(300))
        }
        let statusSeen = Date()
        note("notify status: \"\(statusText)\"")
        guard statusText.hasPrefix("Scheduled") else {
            await finish("banner", terminating: [Self.settingsId])
            throw XCTSkip("no notification could be scheduled (status \"\(statusText)\"): no banner to check")
        }
        let due = dueWindow(app, earliest: scheduledAfter, latest: statusSeen)
        let bannerQuery = board.descendants(matching: .any).matching(identifier: BannerCover.identifier).firstMatch

        // Settings to the front, and really the foreground app, before the notification.
        let settings = XCUIApplication(bundleIdentifier: Self.settingsId)
        let activated = Date()
        settings.activate()
        let inFront = settings.wait(for: .runningForeground, timeout: 15)
        let planner = XCUITestPhoneRunner()
        planner.noteActivated(Self.settingsId)
        let front = try await planner.frontBundleId()
        let remaining = due.earliest.timeIntervalSinceNow
        note("Settings: state \(settings.state.rawValue) \(Self.seconds(Date().timeIntervalSince(activated))) s after activate(); frontBundleId() \(front ?? "nil"); \(Self.seconds(remaining)) s left before the notification is due")
        if !inFront || front != Self.settingsId || remaining < Self.leastMargin {
            let reason = !inFront || front != Self.settingsId
                ? "the race was lost: Settings was not the foreground app (state \(settings.state.rawValue), frontBundleId() \(front ?? "nil")) with the notification due in \(Self.seconds(remaining)) s"
                : "the race was lost: Settings came to the front only \(Self.seconds(remaining)) s before the notification was due (less than \(Self.leastMargin.compactText) s)"
            note(reason)
            await waitOutBanner(due: due.latest, bannerQuery)
            await finish("banner", terminating: [Self.settingsId])
            throw XCTSkip(reason)
        }

        // The look the stale touches are planned with: Settings before the banner.
        let early = bannerQuery.exists
        let before = try await planner.snapshot(includeScreenshot: false)
        let preCovers = before.root.map { Self.findAll($0, Self.isCover) } ?? []
        let preCount = before.root.map { Self.findAll($0) { _ in true }.count } ?? 0
        note("look before the banner (the plan): \(before.appBundleId), \(preCount) nodes, covering nodes \(preCovers.count), banner up by query \(early); notes: \(before.notes.joined(separator: " | "))")
        XCTAssertEqual(before.appBundleId, Self.settingsId, "the look before the banner is not of Settings")
        guard let preRoot = before.root, preCovers.isEmpty, !early else {
            let reason = "the race was lost: a banner was already up (or the look failed) before the look the stale touches are planned with"
            note(reason)
            await waitOutBanner(due: due.latest, bannerQuery)
            await finish("banner", terminating: [Self.settingsId])
            throw XCTSkip(reason)
        }

        // Poll for the banner from before its due time until 15 s after it.
        let pollUntil = due.latest.addingTimeInterval(15)
        note("polling SpringBoard for the banner until 15 s after it is due (\(Self.seconds(pollUntil.timeIntervalSinceNow)) s from now)")
        var firstSeen: Date?
        while Date() < pollUntil {
            if bannerQuery.exists {
                firstSeen = Date()
                break
            }
            try await Task.sleep(for: .milliseconds(300))
        }
        guard let seenAt = firstSeen else {
            note("no banner in SpringBoard's tree by 15 s after the notification was due, with Settings in front")
            await finish("banner", terminating: [Self.settingsId])
            throw XCTSkip("no banner appeared in SpringBoard's tree by 15 s after the notification was due (Settings in front since \(Self.seconds(remaining)) s before it)")
        }
        note("banner first seen by query \(Self.seconds(seenAt.timeIntervalSince(due.latest))) s after its due time")
        // It slides in: let it settle before frames are compared.
        try await Task.sleep(for: .milliseconds(1_000))

        // (i) The runner's look of Settings: exactly one covering node, at the banner's frame.
        let queriedBefore = queryFrame(bannerQuery)
        let looker = XCUITestPhoneRunner()
        looker.noteActivated(Self.settingsId)
        let look = try await looker.snapshot(includeScreenshot: false)
        let queriedAfter = queryFrame(bannerQuery)
        let boardRaw = (try? board.snapshot()).map { SnapshotCopy.copy($0) }
        note("banner by query before the look: \(queriedBefore.map(Self.cgText) ?? "gone"); after it: \(queriedAfter.map(Self.cgText) ?? "gone")")
        note("the look: \(look.appBundleId), screen \(look.screenSize.width.compactText)x\(look.screenSize.height.compactText) pt; notes: \(look.notes.joined(separator: " | "))")
        if let boardRaw {
            note("SpringBoard's snapshot: application node frame \(Self.rectText(boardRaw.frame))")
            let found = Self.rawBanners(boardRaw)
            if found.isEmpty { note("SpringBoard's snapshot: no banner node") }
            for (banner, window) in found {
                note("SpringBoard's snapshot: banner node frame \(Self.rectText(banner.frame)), its window's frame \(window.map { Self.rectText($0.frame) } ?? "none")")
            }
            note("BannerCover.frames (the runner's conversion to points): \(BannerCover.frames(inSpringBoard: boardRaw).map(Self.rectText).joined(separator: ", "))")
        } else {
            note("SpringBoard's snapshot failed")
        }
        let covers = look.root.map { Self.findAll($0, Self.isCover) } ?? []
        note("covering nodes in the look (button, no label, disabled): \(covers.count): \(covers.map { Self.describe($0) }.joined(separator: " ; "))")
        note("the root's last child is a covering node: \(look.root?.children.last.map(Self.isCover) ?? false)")
        XCTAssertEqual(look.appBundleId, Self.settingsId, "the look with the banner up is not of Settings")
        guard let cover = covers.first else {
            let stillUp = bannerQuery.exists
            note(stillUp ? "the banner is up but the look holds no covering node" : "the banner went away before the look")
            if stillUp { XCTFail("a banner is up but the runner's tree holds no covering node") }
            await waitOutBanner(due: due.latest, bannerQuery)
            await finish("banner", terminating: [Self.settingsId])
            if !stillUp { throw XCTSkip("the banner went away before the runner's look") }
            return
        }
        XCTAssertEqual(covers.count, 1, "the look holds \(covers.count) covering nodes, not one")
        XCTAssertLessThanOrEqual(cover.frame.width, look.screenSize.width, "the covering node is in pixels, not points")
        if let q = queriedAfter ?? queriedBefore {
            note("compared with the query's frame \(Self.cgText(q)): left \(Self.seconds(cover.frame.x - Double(q.minX))), top \(Self.seconds(cover.frame.y - Double(q.minY))), right \(Self.seconds(cover.frame.x + cover.frame.width - Double(q.maxX))), bottom \(Self.seconds(cover.frame.y + cover.frame.height - Double(q.maxY))) pt")
            XCTAssertEqual(cover.frame.x, Double(q.minX), accuracy: 4, "the covering node's left edge is not the banner's")
            XCTAssertEqual(cover.frame.y, Double(q.minY), accuracy: 4, "the covering node's top edge is not the banner's")
            XCTAssertEqual(cover.frame.x + cover.frame.width, Double(q.maxX), accuracy: 4, "the covering node's right edge is not the banner's")
            XCTAssertEqual(cover.frame.y + cover.frame.height, Double(q.maxY), accuracy: 4, "the covering node's bottom edge is not the banner's")
        } else {
            unchecked.append("(i) the banner's query frame could not be read")
            note("the banner's query frame could not be read: the frame is not compared")
        }

        // (ii) A tap at the cover's centre planned with the look before the banner: refused
        // as stale, nothing synthesized (the banner stays up, Jarvis does not open).
        let centre = Self.centre(cover.frame)
        if bannerQuery.exists {
            let result = try await planner.perform(.tap(x: centre.x, y: centre.y), expect: CommandExpect(bundleId: Self.settingsId))
            let upAfter = bannerQuery.exists
            let jarvisFront = app.state == .runningForeground
            note("(ii) tap at the banner's centre (\(centre.x.compactText), \(centre.y.compactText)) planned before it: ok \(result.ok), failure \(String(describing: result.failure)), \(result.error ?? ""); banner up after \(upAfter), Jarvis in front \(jarvisFront)")
            XCTAssertFalse(result.ok, "a tap in a banner the look did not hold was carried out")
            XCTAssertEqual(result.failure, RunnerFailure.stale)
            XCTAssertTrue(result.error?.contains("notification banner") ?? false, "not the banner refusal: \(result.error ?? "")")
            XCTAssertTrue(upAfter, "the banner went away after the refused tap: a touch may have reached it")
            XCTAssertFalse(jarvisFront, "Jarvis came to the front: the banner was tapped")
        } else {
            unchecked.append("(ii) the banner went away before the stale tap")
            note("(ii) the banner went away before the stale tap: not checked")
        }

        // (iii) A Settings control under the banner is not tappable with that plan; one well
        // below it still is.
        let screenArea = before.screenSize.width * before.screenSize.height
        let overlapping = Self.findAll(preRoot) { node in
            Self.isPlaced(node) && node.frame.width * node.frame.height < screenArea / 2
                && Self.intersection(node.frame, cover.frame) != nil
        }
        let underNode = overlapping.first(where: Self.isControl) ?? overlapping.first
        let underPoint: (x: Double, y: Double)
        if let underNode, let overlap = Self.intersection(underNode.frame, cover.frame) {
            underPoint = Self.centre(overlap)
            note("(iii) Settings' node under the banner (from the look before it): \(Self.describe(underNode)); tapped at the middle of the overlap")
        } else {
            underPoint = (cover.frame.x + cover.frame.width / 4, cover.frame.y + cover.frame.height / 2)
            note("(iii) no Settings node under the banner in the look before it; a point inside the banner is tapped")
        }
        if bannerQuery.exists {
            let result = try await planner.perform(.tap(x: underPoint.x, y: underPoint.y), expect: CommandExpect(bundleId: Self.settingsId))
            let upAfter = bannerQuery.exists
            let jarvisFront = app.state == .runningForeground
            note("(iii) tap under the banner at (\(underPoint.x.compactText), \(underPoint.y.compactText)): ok \(result.ok), failure \(String(describing: result.failure)), \(result.error ?? ""); banner up after \(upAfter), Jarvis in front \(jarvisFront)")
            XCTAssertFalse(result.ok, "a tap on a Settings control under the banner was carried out")
            XCTAssertEqual(result.failure, RunnerFailure.stale)
            XCTAssertTrue(result.error?.contains("notification banner") ?? false, "not the banner refusal: \(result.error ?? "")")
            XCTAssertTrue(upAfter, "the banner went away after the refused tap: a touch may have reached it")
            XCTAssertFalse(jarvisFront, "Jarvis came to the front: the banner was tapped")
        } else {
            unchecked.append("(iii) the banner went away before the tap under it")
            note("(iii) the banner went away before the tap under it: not checked")
        }
        let coverBottom = cover.frame.y + cover.frame.height
        // Settings' General row when it is there (an ordinary page, no sheet), else the
        // highest such control.
        let belowCandidates = Self.findAll(preRoot) { node in
            Self.isControl(node) && node.enabled && node.frame.height >= 20
                && node.frame.y >= coverBottom + 150
                && node.frame.y + node.frame.height <= before.screenSize.height - 120
        }
        let general = belowCandidates.first(where: { $0.identifier == "com.apple.settings.general" })
        let below = general ?? belowCandidates.min(by: { $0.frame.y < $1.frame.y })
        if let below {
            let point = Self.centre(below.frame)
            let upBefore = bannerQuery.exists
            let result = try await planner.perform(.tap(x: point.x, y: point.y), expect: CommandExpect(bundleId: Self.settingsId))
            note("(iii) tap well below the banner on \(Self.describe(below)): ok \(result.ok), failure \(String(describing: result.failure)), \(result.error ?? ""); banner up before it \(upBefore)")
            XCTAssertTrue(result.ok, "a tap well below the banner was refused: \(result.error ?? "")")
        } else {
            unchecked.append("(iii) no Settings control well below the banner in the look before it")
            note("(iii) no Settings control well below the banner in the look before it: not checked")
        }

        // (iv) Once the banner has gone, a fresh look has no covering node.
        let goneDeadline = Date().addingTimeInterval(12)
        while bannerQuery.exists, Date() < goneDeadline { try await Task.sleep(for: .milliseconds(500)) }
        if bannerQuery.exists {
            unchecked.append("(iv) the banner was still up 12 s after the checks")
            note("(iv) the banner was still up 12 s after the checks: not checked")
        } else {
            note("(iv) the banner has gone (\(Self.seconds(Date().timeIntervalSince(seenAt))) s after it was first seen)")
            let fresh = XCUITestPhoneRunner()
            fresh.noteActivated(Self.settingsId)
            let after = try await fresh.snapshot(includeScreenshot: false)
            let afterCovers = after.root.map { Self.findAll($0, Self.isCover) } ?? []
            note("(iv) fresh look: \(after.appBundleId), covering nodes \(afterCovers.count): \(afterCovers.map { Self.describe($0) }.joined(separator: " ; ")); notes: \(after.notes.joined(separator: " | "))")
            XCTAssertEqual(after.appBundleId, Self.settingsId)
            XCTAssertEqual(afterCovers.count, 0, "a look after the banner has gone still holds a covering node")
            XCTAssertFalse(after.notes.contains { $0.contains("notification banner") }, "a look after the banner has gone still notes a banner")
        }
        await finish("banner", terminating: [Self.settingsId])
        if !unchecked.isEmpty {
            throw XCTSkip("not every check could be made: " + unchecked.joined(separator: "; "))
        }
    }
}
