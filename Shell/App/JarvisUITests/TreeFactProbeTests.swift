import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Evidence gathering, not feature tests: the "device facts" of the safety design
/// (docs/ARCHITECTURE.md, "Open spikes" (a)–(i)) that are facts about iOS's accessibility
/// tree, captured on the simulator. Each probe drives the UI directly with XCUITest (no agent,
/// no session, no model), dumps what is in front at each interesting moment with the
/// harness's own dump (`QARun.treeText`: the typed tree as the core receives it, then the raw
/// snapshot) and writes plain sentences about what it did and found by simple checks.
///
/// Artifacts per probe (`fact-<letter>-<name>`): `<fact>.NN-tree.txt` and `<fact>.NN.png` per
/// capture, `<fact>.NN-query.txt` where XCUITest's query view (`debugDescription`) is recorded
/// too, and one `<fact>.result.txt`. A probe never judges: a missing element is recorded
/// (with the tree dumped anyway) and the probe ends without failing. XCTest issues raised by
/// probing calls are captured as data with `XCTExpectFailure` (non-strict), as the lock probe
/// does. Only harness errors (the app does not launch) fail a probe.
///
/// Every probe ends on the Home Screen, in portrait, with Jarvis, Settings and Safari
/// terminated (`finish`), which is what the other tests assume at their start.
final class TreeFactProbeTests: QATestCase {
    private static let springboardId = "com.apple.springboard"
    private static let settingsId = "com.apple.Preferences"
    private static let safariId = "com.apple.mobilesafari"
    private static let spotlightId = "com.apple.Spotlight"
    /// The body of the probe notification (`ProbeNotifyFixture.bannerBody` in the app).
    private static let bannerBody = "Probe banner 4417"

    private var fact = "fact"
    private var captures = 0
    private var lines: [String] = []
    private var started = Date()
    /// The last observation `runnerView`'s runner reported (main actor only).
    private var runnerRecord: RunnerObservationRecord?

    override func tearDownWithError() throws {
        // Never leave the simulator rotated (the other tests assume portrait), even when a
        // probe ended early. Teardown runs on the main thread; checked, not assumed.
        if Thread.isMainThread {
            MainActor.assumeIsolated {
                if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
            }
        }
        try super.tearDownWithError()
    }

    // MARK: - (a) Notification banner over another app

    @MainActor
    func testFactA_BannerOverAnotherApp() async throws {
        executionTimeAllowance = 240
        begin("fact-a-banner")
        await probeBanner()
        await finish(terminating: [Self.settingsId])
    }

    @MainActor
    private func probeBanner() async {
        let app = launchPlayground()
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        guard await openProbeScreen(app, "probe.notify") else {
            capture(app, Self.appBundleId, "the Notification probe screen was not reached")
            return
        }
        let schedule = app.buttons["probe.notify.schedule"].firstMatch
        guard waitFor(schedule, "the Notify in 6 s button", 5) else {
            capture(app, Self.appBundleId, "no Notify button")
            return
        }
        tolerant("tap Notify in 6 s") { schedule.tap() }
        note("tapped Notify in 6 s: the app asks for notification permission, then schedules a local notification 6 s ahead")

        // The permission alert (first run on a simulator only): SpringBoard's or the app's.
        // Without one (permission already decided) the status changes at once and the
        // notification is already counting down, so the wait ends on that too.
        let status = app.staticTexts["probe.notify.status"].firstMatch
        var alertOwner: (String, XCUIApplication)?
        let alertDeadline = Date().addingTimeInterval(6)
        while alertOwner == nil, Date() < alertDeadline {
            if board.alerts.firstMatch.exists {
                alertOwner = (Self.springboardId, board)
            } else if app.alerts.firstMatch.exists {
                alertOwner = (Self.appBundleId, app)
            } else if status.exists, ["Scheduled", "Not allowed"].contains(status.label) {
                break
            } else {
                await pause(0.3)
            }
        }
        if let (ownerId, owner) = alertOwner {
            note("a permission alert appeared in \(ownerId)'s tree")
            capture(board, Self.springboardId, "notification permission alert up (SpringBoard)")
            capture(app, Self.appBundleId, "notification permission alert up (Jarvis)")
            let alert = owner.alerts.firstMatch
            tolerant("read the permission alert") {
                note("permission alert label \"\(alert.label)\", buttons: \(alert.buttons.allElementsBoundByIndex.map { $0.label })")
            }
            tolerant("tap Allow") {
                let allow = alert.buttons["Allow"].firstMatch
                if allow.exists {
                    allow.tap()
                    note("tapped Allow")
                } else {
                    note("the alert has no button labelled Allow; nothing tapped")
                }
            }
        } else {
            note("no permission alert seen in SpringBoard or Jarvis (permission already decided, or the alert is shown elsewhere)")
        }

        var statusText = "(unread)"
        let statusDeadline = Date().addingTimeInterval(8)
        while Date() < statusDeadline {
            tolerant("read the notify status") { statusText = status.exists ? status.label : "(no status text)" }
            if statusText != "Asking" && statusText != "Not scheduled" && statusText != "(unread)" { break }
            await pause(0.4)
        }
        note("the app's notify status: \"\(statusText)\"")
        guard statusText == "Scheduled" else {
            note("the notification was not scheduled, so no banner can appear; if the status says Not allowed, notification permission could not be obtained in this simulator")
            capture(app, Self.appBundleId, "notification not scheduled")
            return
        }

        let settings = XCUIApplication(bundleIdentifier: Self.settingsId)
        tolerant("open Settings") { settings.activate() }
        let inFront = settings.wait(for: .runningForeground, timeout: 10)
        note(inFront ? "Settings is in front" : "Settings did not reach the foreground in 10 s (state \(settings.state.rawValue))")

        let predicate = NSPredicate(format: "label CONTAINS %@", Self.bannerBody)
        let inBoard = board.descendants(matching: .any).matching(predicate).firstMatch
        let inSettings = settings.descendants(matching: .any).matching(predicate).firstMatch
        var seenIn = ""
        let bannerDeadline = Date().addingTimeInterval(15)
        while seenIn.isEmpty, Date() < bannerDeadline {
            if inBoard.exists {
                seenIn = "SpringBoard"
            } else if inSettings.exists {
                seenIn = "Settings"
            } else {
                await pause(0.3)
            }
        }
        note(seenIn.isEmpty
             ? "no node whose label contains \"\(Self.bannerBody)\" appeared in SpringBoard's or Settings' tree within 15 s (polled with XCUITest queries every 0.3 s); trees captured anyway"
             : "a node whose label contains \"\(Self.bannerBody)\" was first found by query in \(seenIn)'s tree")
        let frontTree = capture(settings, Self.settingsId, "banner expected up: the front app (Settings)")
        let boardTree = capture(board, Self.springboardId, "banner expected up: SpringBoard")
        let front = await frontApp()
        note("front app by the runner's reading: \(front)")
        report(frontTree, "Settings' tree: nodes whose label, value or id contains \"\(Self.bannerBody)\"", Self.mentions(Self.bannerBody))
        report(frontTree, "Settings' tree: nodes mentioning \"Jarvis probe\" (the notification's title)", Self.mentions("Jarvis probe"))
        report(boardTree, "SpringBoard's tree: nodes whose label, value or id contains \"\(Self.bannerBody)\"", Self.mentions(Self.bannerBody))
        report(boardTree, "SpringBoard's tree: nodes mentioning \"Jarvis probe\" (the notification's title)", Self.mentions("Jarvis probe"))

        // Let the banner go before leaving (it would cover the next test's first screen).
        let goneDeadline = Date().addingTimeInterval(10)
        while inBoard.exists, Date() < goneDeadline { await pause(0.5) }
        note(inBoard.exists ? "the banner node was still in SpringBoard's tree after 10 s" : "no banner node in SpringBoard's tree any more")
    }

    // MARK: - (b) In-app browser sheets

    @MainActor
    func testFactB_InAppBrowserSheets() async throws {
        executionTimeAllowance = 240
        begin("fact-b-browser-sheets")
        await probeBrowserSheets()
        await finish(terminating: [])
    }

    @MainActor
    private func probeBrowserSheets() async {
        let server: FormCaptureServer
        do {
            server = try FormCaptureServer()
        } catch {
            note("the local test server did not start: \(error)")
            return
        }
        defer { server.stop() }
        let url = server.baseURL.absoluteString + "/"
        note("local page \(url), passed to the app as -jarvis-probe-url")
        let arguments = ["-jarvis-probe-url", url]
        let app = launchPlayground(extraArguments: arguments)
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        guard await openProbeScreen(app, "probe.sheets") else {
            capture(app, Self.appBundleId, "the Browser sheets probe screen was not reached")
            return
        }
        let auth = app.buttons["probe.sheets.auth"].firstMatch

        // 1. SFSafariViewController.
        let open = app.buttons["probe.sheets.safari"].firstMatch
        if waitFor(open, "the Open in-app browser button", 5) {
            tolerant("tap Open in-app browser") { open.tap() }
            let heading = Self.node(in: app, labelled: "Contact form")
            note(waitFor(heading, "the page heading \"Contact form\" in Jarvis's tree", 15)
                 ? "the page heading \"Contact form\" was found by query in Jarvis's tree"
                 : "the page heading was not found in Jarvis's tree within 15 s")
            await pause(1)
            let sheet = capture(app, Self.appBundleId, "SFSafariViewController presented (Jarvis)")
            capture(board, Self.springboardId, "SFSafariViewController presented (SpringBoard)")
            let front = await frontApp()
            note("front app by the runner's reading: \(front)")
            reportBrowser(sheet, "SFSafariViewController, Jarvis's tree")
            attachQuery(app, "SFSafariViewController presented (Jarvis's query view)")
            // What the queries reach, next to what the snapshot holds.
            let queried: [(String, XCUIElement)] = [
                ("the page heading \"Contact form\"", heading),
                ("button Close", app.buttons["Close"].firstMatch),
                ("button Done", app.buttons["Done"].firstMatch),
            ]
            var closeFrame: CGRect?
            for (what, element) in queried {
                if let frame = queryFrame(element) {
                    note("by query: \(what) exists, frame \(Self.frameText(frame))")
                    if closeFrame == nil, what.hasPrefix("button") { closeFrame = frame }
                } else {
                    note("by query: no \(what)")
                }
            }
            report(sheet, "the snapshot above (capture of Jarvis): nodes labelled Contact form, Close or Done (compare with the queries just above)") { e in
                ["Contact form", "Close", "Done"].contains(e.label)
            }
            await runnerView("SFSafariViewController up")
            guard await dismissBrowser(app, auth, closeFrame: closeFrame, arguments: arguments) else { return }
        }

        // 2. ASWebAuthenticationSession: a consent alert first, then the sheet.
        guard waitFor(auth, "the Start web sign-in button", 5) else {
            capture(app, Self.appBundleId, "no Start web sign-in button (the in-app browser may still be up)")
            return
        }
        tolerant("tap Start web sign-in") { auth.tap() }
        var alertOwner: (String, XCUIApplication)?
        let alertDeadline = Date().addingTimeInterval(8)
        while alertOwner == nil, Date() < alertDeadline {
            if app.alerts.firstMatch.exists {
                alertOwner = (Self.appBundleId, app)
            } else if board.alerts.firstMatch.exists {
                alertOwner = (Self.springboardId, board)
            } else {
                await pause(0.3)
            }
        }
        let alertApp = capture(app, Self.appBundleId, "web sign-in consent alert expected (Jarvis)")
        let alertBoard = capture(board, Self.springboardId, "web sign-in consent alert expected (SpringBoard)")
        report(alertApp, "Jarvis's tree: Alert nodes") { $0.elementType == XCUIElementTypeCode.alert }
        report(alertBoard, "SpringBoard's tree: Alert nodes") { $0.elementType == XCUIElementTypeCode.alert }
        reportInsideAlert(alertApp, "Jarvis's tree")
        reportInsideAlert(alertBoard, "SpringBoard's tree")
        if let (ownerId, owner) = alertOwner {
            note("the consent alert was found by query in \(ownerId)'s tree")
            tolerant("read the consent alert by query") {
                let alert = owner.alerts.firstMatch
                let texts = alert.staticTexts.allElementsBoundByIndex.map { "\"\(Self.clip($0.label))\"" }
                let buttons = alert.buttons.allElementsBoundByIndex.map { "\"\($0.label)\"" }
                note("consent alert (\(ownerId)): label \"\(Self.clip(alert.label))\", frame \(Self.frameText(alert.frame)); texts: \(texts.joined(separator: ", ")); buttons: \(buttons.joined(separator: ", "))")
            }
            tolerant("tap Continue on the consent alert") {
                let button = owner.alerts.firstMatch.buttons["Continue"].firstMatch
                if button.exists {
                    button.tap()
                    note("tapped Continue")
                } else {
                    note("the consent alert has no Continue button; buttons: \(owner.alerts.firstMatch.buttons.allElementsBoundByIndex.map { $0.label })")
                }
            }
        } else {
            note("no consent alert in Jarvis's or SpringBoard's tree within 8 s")
        }
        let heading = Self.node(in: app, labelled: "Contact form")
        note(waitFor(heading, "the page heading in the web sign-in sheet", 15)
             ? "the page heading was found by query in Jarvis's tree (web sign-in sheet)"
             : "the page heading was not found in Jarvis's tree within 15 s (web sign-in sheet)")
        await pause(1)
        let authSheet = capture(app, Self.appBundleId, "web sign-in sheet up (Jarvis)")
        capture(board, Self.springboardId, "web sign-in sheet up (SpringBoard)")
        let front = await frontApp()
        note("front app by the runner's reading: \(front)")
        reportBrowser(authSheet, "web sign-in sheet, Jarvis's tree")
        attachQuery(app, "web sign-in sheet up (Jarvis's query view)")
        for (what, element) in [("the page heading \"Contact form\"", heading), ("button Cancel", app.buttons["Cancel"].firstMatch)] {
            if let frame = queryFrame(element) {
                note("by query (web sign-in sheet): \(what) exists, frame \(Self.frameText(frame))")
            } else {
                note("by query (web sign-in sheet): no \(what)")
            }
        }
        tolerant("cancel the web sign-in") {
            let cancel = app.buttons["Cancel"].firstMatch
            if cancel.exists { cancel.tap() } else { note("no Cancel button in Jarvis's tree") }
        }
        await pause(1.5)
        var status = "(unread)"
        tolerant("read the sheets status") {
            let text = app.staticTexts["probe.sheets.status"].firstMatch
            status = text.exists ? text.label : "(no status text: the sheet may still be up)"
        }
        note("after the cancel, the probe screen's status: \"\(status)\"")
        let log = server.log
        note("server log: " + (log.isEmpty ? "(no request reached the server)" : log.joined(separator: " | ")))
    }

    @MainActor
    private func reportBrowser(_ tree: RawElement?, _ what: String) {
        report(tree, "\(what): WebView nodes") { $0.elementType == XCUIElementTypeCode.webView }
        report(tree, "\(what): Sheet or Other nodes with an identifier") { e in
            (e.elementType == XCUIElementTypeCode.sheet || e.elementType == XCUIElementTypeCode.other) && !e.identifier.isEmpty
        }
        report(tree, "\(what): nodes naming the host 127.0.0.1", Self.mentions("127.0.0.1"))
        report(tree, "\(what): nodes with Address or URL in label or id") { e in
            e.label.localizedCaseInsensitiveContains("address") || e.identifier.localizedCaseInsensitiveContains("address")
                || e.identifier.localizedCaseInsensitiveContains("url")
        }
        report(tree, "\(what): text inputs") { e in
            [XCUIElementTypeCode.textField, XCUIElementTypeCode.searchField, XCUIElementTypeCode.textView].contains(e.elementType)
        }
        report(tree, "\(what): the page heading", Self.mentions("Contact form"))
        report(tree, "\(what): buttons Done, Cancel, Close") { e in
            e.elementType == XCUIElementTypeCode.button && ["Done", "Cancel", "Close"].contains(e.label)
        }
    }

    /// Closes the in-app browser, each step verified by the probe screen's own Start web sign-in
    /// button being hittable again: the Close/Done button, a coordinate tap at the queried close
    /// control, one at the top-left corner, a drag down, and last a relaunch of Jarvis with the
    /// same arguments. Returns whether the probe screen is back.
    @MainActor
    private func dismissBrowser(_ app: XCUIApplication, _ auth: XCUIElement, closeFrame: CGRect?, arguments: [String]) async -> Bool {
        var tapped = ""
        tolerant("tap Close (or Done)") {
            for label in ["Close", "Done"] where tapped.isEmpty {
                let button = app.buttons[label].firstMatch
                if button.exists {
                    button.tap()
                    tapped = label
                }
            }
        }
        note(tapped.isEmpty ? "no Close or Done button by query; nothing tapped" : "tapped \(tapped) (XCUIElement.tap) to close the in-app browser")
        if await probeScreenBack(auth, "after the \(tapped.isEmpty ? "missing" : tapped) button") { return true }

        if let frame = closeFrame, frame.width > 0, frame.height > 0 {
            let x = frame.midX
            let y = frame.midY
            tolerant("coordinate tap at the queried close control") {
                app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0)).withOffset(CGVector(dx: x, dy: y)).tap()
            }
            note("coordinate tap at the centre of the queried close control (\(Int(x)), \(Int(y)))")
            if await probeScreenBack(auth, "after the coordinate tap at the queried close control") { return true }
        }

        tolerant("coordinate tap at the top-left close control") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.09, dy: 0.088)).tap()
        }
        note("coordinate tap at the top-left corner (9% of the width, 8.8% of the height)")
        if await probeScreenBack(auth, "after the coordinate tap at the top-left corner") { return true }

        drag(app, from: 0.15, to: 0.95, "drag the in-app browser down")
        if await probeScreenBack(auth, "after the drag down") { return true }

        capture(app, Self.appBundleId, "the in-app browser still up after every way of closing it")
        note("the in-app browser could not be closed: relaunching Jarvis with the same arguments")
        _ = launchPlayground(extraArguments: arguments)
        guard await openProbeScreen(app, "probe.sheets") else {
            capture(app, Self.appBundleId, "the Browser sheets probe screen was not reached after the relaunch")
            return false
        }
        return await probeScreenBack(auth, "after the relaunch")
    }

    /// Whether `button` (a control of the probe screen under a sheet) exists and is hittable.
    @MainActor
    private func probeScreenBack(_ button: XCUIElement, _ when: String) async -> Bool {
        await pause(1)
        var back = false
        tolerant("check that the probe screen is back") {
            back = button.waitForExistence(timeout: 3) && button.isHittable
        }
        note("\(when): the Start web sign-in button is " + (back ? "hittable (the probe screen is back)" : "not hittable (the sheet is still up)"))
        return back
    }

    /// Static texts and buttons that sit inside a raw Alert node of `tree`.
    @MainActor
    private func reportInsideAlert(_ tree: RawElement?, _ what: String) {
        guard let tree else {
            note("\(what): texts and buttons inside an Alert: no tree (the capture failed)")
            return
        }
        let candidates = Self.hits(tree, { e in
            e.elementType == XCUIElementTypeCode.staticText || e.elementType == XCUIElementTypeCode.button
        })
        let inside = candidates.filter { hit in
            hit.ancestors.contains(where: { $0.elementType == XCUIElementTypeCode.alert })
        }
        note("\(what): texts and buttons inside an Alert: \(inside.count)")
        for hit in inside.prefix(12) {
            lines.append("        - " + Self.describe(hit, screen: tree.frame))
        }
    }

    // MARK: - (c) Edit menu and context menu

    @MainActor
    func testFactC_EditAndContextMenus() async throws {
        executionTimeAllowance = 240
        begin("fact-c-menus")
        await probeMenus()
        await finish(terminating: [])
    }

    @MainActor
    private func probeMenus() async {
        let app = launchPlayground()
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        guard await openProbeScreen(app, "probe.menus") else {
            capture(app, Self.appBundleId, "the Menus probe screen was not reached")
            return
        }
        let field = app.textFields["probe.menus.field"].firstMatch
        guard waitFor(field, "the probe text field", 5) else {
            capture(app, Self.appBundleId, "no probe text field")
            return
        }
        tolerant("tap the text field") { field.tap() }
        await pause(1)
        tolerant("double-tap the text field") { field.doubleTap() }
        await pause(1.2)
        let first = capture(app, Self.appBundleId, "after a double-tap on the text field (edit menu expected)")
        let firstBoard = capture(board, Self.springboardId, "after a double-tap on the text field (SpringBoard)")
        reportMenus(first, "edit menu, Jarvis's tree")
        reportMenus(firstBoard, "edit menu, SpringBoard's tree")
        reportMenuPage(first, "edit menu, page 1")

        var menuCount = 0
        tolerant("count app.menuItems") { menuCount = app.menuItems.count }
        if menuCount == 0 {
            note("no menu item by query after the double-tap; trying a long press on the field")
            tolerant("long-press the text field") { field.press(forDuration: 1.0) }
            await pause(1.2)
            let long = capture(app, Self.appBundleId, "after a long press on the text field")
            reportMenus(long, "edit menu after a long press, Jarvis's tree")
            reportMenuPage(long, "edit menu after a long press, page 1")
        }
        await pageMenu(app, "edit menu")

        // Select All may sit on an earlier page than the last one shown: page back to it.
        let selectAll = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Select All")).firstMatch
        var backTaps = 0
        while !existsNow(selectAll), backTaps < 6 {
            let back = app.buttons.matching(NSPredicate(format: "label == %@", "Back")).firstMatch
            var usable = false
            tolerant("look for the menu's Back button") { usable = back.exists && back.isEnabled }
            guard usable else { break }
            tolerant("tap the menu's Back button") { back.tap() }
            backTaps += 1
            await pause(0.8)
        }
        if existsNow(selectAll) {
            note("\"Select All\" is in the menu after \(backTaps) tap(s) on the menu's Back button")
            tolerant("tap Select All") { selectAll.tap() }
            await pause(1.2)
            let all = capture(app, Self.appBundleId, "after Select All (edit menu for the whole text expected)")
            reportMenus(all, "edit menu after Select All, Jarvis's tree")
            reportMenuPage(all, "menu after Select All, page 1")
            await pageMenu(app, "menu after Select All")
        } else {
            note("no \"Select All\" node found on any page (after \(backTaps) Back taps); not tapped")
        }

        // Context menu.
        tolerant("dismiss the edit menu (tap the navigation title)") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.1)).tap()
        }
        await pause(0.8)
        let context = app.descendants(matching: .any)["probe.menus.context"].firstMatch
        guard waitFor(context, "the context-menu view", 5) else {
            capture(app, Self.appBundleId, "no context-menu view")
            return
        }
        tolerant("long-press the context-menu view") { context.press(forDuration: 1.2) }
        await pause(1.2)
        let menu = capture(app, Self.appBundleId, "context menu up (Jarvis)")
        let menuBoard = capture(board, Self.springboardId, "context menu up (SpringBoard)")
        reportMenus(menu, "context menu, Jarvis's tree")
        reportMenus(menuBoard, "context menu, SpringBoard's tree")
        attachQuery(app, "context menu up (Jarvis's query view)")
        queryMenuItems(app, "context menu")
        tolerant("dismiss the context menu") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.08)).tap()
        }
    }

    private static let menuLabels: Set<String> = [
        "Cut", "Copy", "Paste", "Select", "Select All", "Look Up", "Translate", "Share…", "Share...",
        "Find Selection", "AutoFill", "Format", "Writing Tools", "Replace…", "Search Web",
        "Probe first action", "Probe second action",
    ]

    @MainActor
    private func reportMenus(_ tree: RawElement?, _ what: String) {
        report(tree, "\(what): nodes labelled exactly as an edit or context menu item", limit: 20) { Self.menuLabels.contains($0.label) }
        report(tree, "\(what): nodes of raw type Menu, MenuItem, MenuButton, Popover or Sheet") { e in
            [XCUIElementTypeCode.menu, XCUIElementTypeCode.menuItem, XCUIElementTypeCode.menuButton,
             XCUIElementTypeCode.popover, XCUIElementTypeCode.sheet].contains(e.elementType)
        }
        report(tree, "\(what): nodes whose raw type or identifier contains \"menu\" or \"callout\" (the probe's own probe.* identifiers and navigation bars left out)", Self.foreignMenuish)
    }

    /// One page of an edit menu as the snapshot holds it: its items and its paging buttons
    /// (raw type, typed role, label, identifier and frame are in each line).
    @MainActor
    private func reportMenuPage(_ tree: RawElement?, _ what: String) {
        report(tree, "\(what): MenuItem nodes", limit: 20) { $0.elementType == XCUIElementTypeCode.menuItem }
        report(tree, "\(what): the menu's paging buttons (Button Back or Forward)") { e in
            e.elementType == XCUIElementTypeCode.button && (e.label == "Back" || e.label == "Forward")
        }
    }

    /// Pages the edit menu with its own Forward button and captures every page, until Forward
    /// is gone or disabled, a page repeats, or `maxPages` pages were seen (page 1 is the
    /// caller's capture).
    @MainActor
    private func pageMenu(_ app: XCUIApplication, _ title: String, maxPages: Int = 6) async {
        var previous = queryMenuItems(app, "\(title), page 1")
        var page = 1
        while page < maxPages {
            let forward = app.buttons.matching(NSPredicate(format: "label == %@", "Forward")).firstMatch
            var usable = false
            var frame = "-"
            tolerant("look for the menu's Forward button") {
                usable = forward.exists && forward.isEnabled
                if forward.exists { frame = Self.frameText(forward.frame) }
            }
            guard usable else {
                note("\(title): no enabled Forward button after page \(page); paging ends")
                return
            }
            tolerant("tap the menu's Forward button") { forward.tap() }
            note("\(title): tapped Forward (query frame \(frame))")
            await pause(0.8)
            page += 1
            let tree = capture(app, Self.appBundleId, "\(title), page \(page)")
            reportMenuPage(tree, "\(title), page \(page)")
            let labels = queryMenuItems(app, "\(title), page \(page)")
            if labels == previous {
                note("\(title): page \(page) holds the same items as the page before; paging ends")
                return
            }
            previous = labels
        }
        note("\(title): stopped after \(maxPages) pages")
    }

    /// What `app.menuItems.allElementsBoundByIndex` reports: labels, identifiers and frames.
    @MainActor
    @discardableResult
    private func queryMenuItems(_ app: XCUIApplication, _ what: String) -> [String] {
        var labels: [String] = []
        var rows: [String] = []
        tolerant("query app.menuItems") {
            for item in app.menuItems.allElementsBoundByIndex {
                let label = item.label
                labels.append(label)
                rows.append("\"\(Self.clip(label))\" id " + (item.identifier.isEmpty ? "-" : Self.clip(item.identifier)) + " frame " + Self.frameText(item.frame))
            }
        }
        note("\(what): app.menuItems.allElementsBoundByIndex reports \(labels.count)")
        for row in rows { lines.append("        - " + row) }
        return labels
    }

    /// `element.exists`, read tolerantly.
    @MainActor
    private func existsNow(_ element: XCUIElement) -> Bool {
        var found = false
        tolerant("check existence") { found = element.exists }
        return found
    }

    // MARK: - (g) Share sheet

    @MainActor
    func testFactG_ShareSheet() async throws {
        executionTimeAllowance = 240
        begin("fact-g-share-sheet")
        await probeShareSheet()
        await finish(terminating: [])
    }

    @MainActor
    private func probeShareSheet() async {
        let server: FormCaptureServer
        do {
            server = try FormCaptureServer()
        } catch {
            note("the local test server did not start: \(error)")
            return
        }
        defer { server.stop() }
        let url = server.baseURL.absoluteString + "/"
        note("local page \(url), passed to the app as -jarvis-probe-url; the probe shares that URL and a small generated image (probe.share.rich)")
        let app = launchPlayground(extraArguments: ["-jarvis-probe-url", url])
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        guard await openProbeScreen(app, "probe.share") else {
            capture(app, Self.appBundleId, "the Share sheet probe screen was not reached")
            return
        }
        let open = app.buttons["probe.share.rich"].firstMatch
        guard waitFor(open, "the Share link and image button", 5) else {
            capture(app, Self.appBundleId, "no Share link and image button")
            return
        }
        tolerant("tap Share link and image") { open.tap() }
        let marker = NSPredicate(format: "label == %@ OR label == %@", "AirDrop", "Copy")
        let inApp = app.descendants(matching: .any).matching(marker).firstMatch
        let inBoard = board.descendants(matching: .any).matching(marker).firstMatch
        var seenIn = ""
        let deadline = Date().addingTimeInterval(8)
        while seenIn.isEmpty, Date() < deadline {
            if inApp.exists {
                seenIn = "Jarvis"
            } else if inBoard.exists {
                seenIn = "SpringBoard"
            } else {
                await pause(0.3)
            }
        }
        note(seenIn.isEmpty
             ? "no node labelled AirDrop or Copy was found by query in Jarvis's or SpringBoard's tree within 8 s"
             : "a node labelled AirDrop or Copy was first found by query in \(seenIn)'s tree")
        await pause(1)
        let presented = capture(app, Self.appBundleId, "share sheet as presented (Jarvis)")
        reportShare(presented, "as presented, Jarvis's tree")
        reportShare(capture(board, Self.springboardId, "share sheet as presented (SpringBoard)"), "as presented, SpringBoard's tree")
        attachQuery(app, "share sheet as presented (Jarvis's query view)")
        await runnerView("share sheet up")
        frameTable(app, presented, "as presented")
        note("the \"on screen\" in the node lines compares a node's own frame with the screen; frames inside the remote container are local to it (see the frame tables), so \"on screen yes\" tells nothing for those nodes")

        // Expand: press on the grabber area, then drag up.
        let collection = app.collectionViews["activityCollectionView"].firstMatch
        var frameBefore = queryFrame(collection)
        var shotBefore = XCUIScreen.main.screenshot().pngRepresentation
        tolerant("press and drag the sheet's grabber up") {
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.60))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.10))
            start.press(forDuration: 0.3, thenDragTo: end)
        }
        note("pressed for 0.3 s at 60% of the height (the grabber area) and dragged to 10%")
        await pause(1.5)
        var frameAfter = queryFrame(collection)
        var shotAfter = XCUIScreen.main.screenshot().pngRepresentation
        noteChange("the press-and-drag", frameBefore, frameAfter, shotBefore, shotAfter)
        let expanded = capture(app, Self.appBundleId, "after the press-and-drag on the grabber (Jarvis)")
        reportShare(expanded, "after the press-and-drag, Jarvis's tree")
        frameTable(app, expanded, "after the press-and-drag")

        // Scroll the list.
        frameBefore = frameAfter
        shotBefore = shotAfter
        tolerant("swipe up on activityCollectionView") { collection.swipeUp() }
        note("swiped up on the collection view found by its identifier activityCollectionView")
        await pause(1.5)
        frameAfter = queryFrame(collection)
        shotAfter = XCUIScreen.main.screenshot().pngRepresentation
        noteChange("the swipe up on the list", frameBefore, frameAfter, shotBefore, shotAfter)
        let scrolled = capture(app, Self.appBundleId, "after scrolling the list (Jarvis)")
        reportShare(scrolled, "after scrolling, Jarvis's tree")
        reportShare(capture(board, Self.springboardId, "after scrolling the list (SpringBoard)"), "after scrolling, SpringBoard's tree")
        frameTable(app, scrolled, "after scrolling")

        tolerant("close the share sheet") {
            let close = app.buttons["Close"].firstMatch
            if close.exists { close.tap() } else { note("no Close button in Jarvis's tree; the app is terminated at the end instead") }
        }
    }

    /// Whether a gesture changed the collection view's queried frame or the screen.
    @MainActor
    private func noteChange(_ gesture: String, _ frameBefore: CGRect?, _ frameAfter: CGRect?, _ shotBefore: Data, _ shotAfter: Data) {
        let before = frameBefore.map { Self.frameText($0) } ?? "none"
        let after = frameAfter.map { Self.frameText($0) } ?? "none"
        let frameChanged = before != after ? "yes" : "no"
        let shotChanged = shotBefore != shotAfter ? "yes" : "no"
        note("\(gesture): activityCollectionView frame by query \(before) -> \(after) (changed: \(frameChanged)); screenshot changed: \(shotChanged) (checksum \(Self.checksum(shotBefore)) -> \(Self.checksum(shotAfter)); the status bar clock alone can change it)")
    }

    /// For the share sheet's containers and rows found by query: the frame the query reports
    /// next to the frame of the node with the same raw type, label and identifier in the
    /// snapshot `tree`, and their difference (query minus snapshot).
    @MainActor
    private func frameTable(_ app: XCUIApplication, _ tree: RawElement?, _ what: String) {
        let collection = app.collectionViews["activityCollectionView"].firstMatch
        var elements: [(String, XCUIElement)] = [
            ("app.popovers[0]", app.popovers.firstMatch),
            ("app.otherElements[ActivityListView]", app.otherElements["ActivityListView"].firstMatch),
            ("app.otherElements[ShareSheet.RemoteContainerView]", app.otherElements["ShareSheet.RemoteContainerView"].firstMatch),
            ("app.collectionViews[activityCollectionView]", collection),
            ("the first node labelled AirDrop", Self.node(in: app, labelled: "AirDrop")),
        ]
        tolerant("list app.cells") {
            for (i, cell) in app.cells.allElementsBoundByIndex.prefix(12).enumerated() {
                elements.append(("app.cells[\(i)]", cell))
            }
        }
        tolerant("list the collection view's buttons") {
            for (i, button) in collection.buttons.allElementsBoundByIndex.prefix(4).enumerated() {
                elements.append(("activityCollectionView.buttons[\(i)]", button))
            }
        }
        var nodes: [Hit] = []
        if let tree { nodes = Self.hits(tree, { _ in true }) }
        let container = nodes.first(where: { $0.element.identifier == "ShareSheet.RemoteContainerView" })?.element.frame
        let containerText = container.map { Self.rectText($0) } ?? "not in the snapshot"
        lines.append("\(what): frames by query next to the snapshot's (difference = query - snapshot; the remote container's snapshot frame is \(containerText)):")
        for (name, element) in elements {
            var row = "\(name): unreadable"
            tolerant("read \(name) by query") {
                guard element.exists else {
                    row = "\(name): not found by query"
                    return
                }
                let type = element.elementType.rawValue
                let label = element.label
                let identifier = element.identifier
                let frame = element.frame
                var text = "\(name): \(XCUIElementTypeCode.name(type))"
                if !label.isEmpty { text += " \"\(Self.clip(label))\"" }
                if !identifier.isEmpty { text += " id \(Self.clip(identifier))" }
                text += " query " + Self.frameText(frame)
                let match = nodes.first(where: { n in
                    n.element.elementType == type && n.element.label == label && n.element.identifier == identifier
                })
                if let snapped = match?.element.frame {
                    let dx = Double(frame.origin.x) - snapped.x
                    let dy = Double(frame.origin.y) - snapped.y
                    text += " snapshot " + Self.rectText(snapped) + String(format: " difference %+.0f,%+.0f", dx, dy)
                    if let c = container {
                        text += String(format: " (difference less the container's snapshot origin %+.0f,%+.0f)", dx - c.x, dy - c.y)
                    }
                    if abs(Double(frame.width) - snapped.width) > 0.5 || abs(Double(frame.height) - snapped.height) > 0.5 {
                        text += "; sizes differ"
                    }
                } else {
                    text += " snapshot: no node of that raw type, label and identifier"
                }
                row = text
            }
            lines.append("        - " + row)
        }
    }

    @MainActor
    private func reportShare(_ tree: RawElement?, _ what: String) {
        report(tree, "\(what): nodes labelled AirDrop") { $0.label == "AirDrop" }
        report(tree, "\(what): nodes labelled Copy") { $0.label == "Copy" }
        report(tree, "\(what): nodes of raw type Sheet") { $0.elementType == XCUIElementTypeCode.sheet }
        report(tree, "\(what): Cell nodes", limit: 6) { $0.elementType == XCUIElementTypeCode.cell }
        report(tree, "\(what): nodes whose identifier mentions Remote, Share or Activity") { e in
            ["remote", "share", "activity"].contains { e.identifier.localizedCaseInsensitiveContains($0) }
        }
    }

    // MARK: - (h) Picker, slider, switch, stepper

    @MainActor
    func testFactH_ControlRoles() async throws {
        begin("fact-h-controls")
        await probeControls()
        await finish(terminating: [])
    }

    @MainActor
    private func probeControls() async {
        let app = launchPlayground()
        guard await openProbeScreen(app, "probe.controls") else {
            capture(app, Self.appBundleId, "the Controls probe screen was not reached")
            return
        }
        let picker = app.descendants(matching: .any)["probe.controls.picker"].firstMatch
        _ = waitFor(picker, "the wheel picker", 5)
        await pause(0.8)
        reportControls(capture(app, Self.appBundleId, "controls screen, top"), "top of the screen")
        drag(app, from: 0.8, to: 0.3, "scroll the form up")
        await pause(1.2)
        reportControls(capture(app, Self.appBundleId, "controls screen, scrolled"), "after scrolling")
        note("traits (\"adjustable\"): an XCUITest snapshot carries no accessibility traits, so the dump cannot show them; the raw type is the only role information")
    }

    @MainActor
    private func reportControls(_ tree: RawElement?, _ what: String) {
        report(tree, "\(what): Picker, PickerWheel and DatePicker nodes") { e in
            [XCUIElementTypeCode.picker, XCUIElementTypeCode.pickerWheel, XCUIElementTypeCode.datePicker].contains(e.elementType)
        }
        report(tree, "\(what): Slider nodes") { $0.elementType == XCUIElementTypeCode.slider }
        report(tree, "\(what): Switch and Toggle nodes") { e in
            e.elementType == XCUIElementTypeCode.`switch` || e.elementType == XCUIElementTypeCode.toggle
        }
        report(tree, "\(what): Stepper, IncrementArrow and DecrementArrow nodes") { e in
            [XCUIElementTypeCode.stepper, XCUIElementTypeCode.incrementArrow, XCUIElementTypeCode.decrementArrow].contains(e.elementType)
        }
        report(tree, "\(what): nodes with a probe.controls identifier", limit: 20) { $0.identifier.hasPrefix("probe.controls") }
        report(tree, "\(what): nodes mentioning Probe (labels of the controls)", limit: 20, Self.mentions("Probe"))
        report(tree, "\(what): scroll containers (ScrollView, Table, CollectionView, WebView), with or without a label") { e in
            [XCUIElementTypeCode.scrollView, XCUIElementTypeCode.table, XCUIElementTypeCode.collectionView,
             XCUIElementTypeCode.webView].contains(e.elementType)
        }
        report(tree, "\(what): scroll containers with a non-empty label") { e in
            [XCUIElementTypeCode.scrollView, XCUIElementTypeCode.table, XCUIElementTypeCode.collectionView,
             XCUIElementTypeCode.webView].contains(e.elementType) && !e.label.isEmpty
        }
    }

    // MARK: - Spotlight rows

    @MainActor
    func testFactSpotlightRows() async throws {
        begin("fact-spotlight-rows")
        await probeSpotlight()
        await finish(terminating: [])
    }

    @MainActor
    private func probeSpotlight() async {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let spotlight = XCUIApplication(bundleIdentifier: Self.spotlightId)
        drag(board, from: 0.4, to: 0.75, "swipe down on the Home Screen (Spotlight)")
        await pause(1.5)
        note("Spotlight (\(Self.spotlightId)) state \(stateText(spotlight)), SpringBoard state \(stateText(board))")
        var typist: (String, XCUIApplication)?
        let deadline = Date().addingTimeInterval(4)
        while typist == nil, Date() < deadline {
            typist = keyboardOwner([(Self.spotlightId, spotlight), (Self.springboardId, board)])
            if typist == nil { await pause(0.4) }
        }
        guard let (typistId, typingApp) = typist else {
            note("no software keyboard in Spotlight's or SpringBoard's tree within 4 s; nothing typed")
            captureSpotlight(spotlight, board, "no keyboard after the swipe down")
            return
        }
        note("the keyboard is in \(typistId)'s tree; typing there")
        tolerant("type Settings") { typingApp.typeText("Settings") }
        await pause(2)
        captureSpotlight(spotlight, board, "Spotlight after typing \"Settings\"", query: "Settings")
        // What Jarvis's own runner sees while Spotlight is up (the code the agent runs).
        await runnerView("Spotlight up with \"Settings\" typed", query: "Settings")
        tolerant("clear the query") { typingApp.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 10)) }
        tolerant("type example.com") { typingApp.typeText("example.com") }
        await pause(2)
        captureSpotlight(spotlight, board, "Spotlight after typing \"example.com\"", query: "example")
        tolerant("press Home to leave Spotlight") { XCUIDevice.shared.press(.home) }
        await pause(1)
    }

    @MainActor
    private func captureSpotlight(_ spotlight: XCUIApplication, _ board: XCUIApplication, _ moment: String, query: String? = nil) {
        var trees: [(String, RawElement?)] = []
        if isRunning(spotlight) {
            trees.append(("Spotlight", capture(spotlight, Self.spotlightId, moment + " (Spotlight)")))
        } else {
            note("Spotlight is not running (state \(stateText(spotlight))); its tree is not captured")
        }
        trees.append(("SpringBoard", capture(board, Self.springboardId, moment + " (SpringBoard)")))
        for (owner, tree) in trees {
            report(tree, "\(owner)'s tree: Cell nodes (result rows)", limit: 15) { $0.elementType == XCUIElementTypeCode.cell }
            report(tree, "\(owner)'s tree: SearchField and TextField nodes") { e in
                e.elementType == XCUIElementTypeCode.searchField || e.elementType == XCUIElementTypeCode.textField
            }
            if let query {
                report(tree, "\(owner)'s tree: nodes mentioning \"\(query)\"", limit: 15, Self.mentions(query))
            }
        }
    }

    // MARK: - Home Screen icon menu

    @MainActor
    func testFactHomeScreenIconMenu() async throws {
        // XCUITest waits for SpringBoard to idle after the long press (60 s in round 1: "App
        // animations complete notification not received"); no way around that wait is used in
        // this repo, so the probe is given room for it.
        executionTimeAllowance = 180
        begin("fact-home-icon-menu")
        await probeIconMenu()
        await finish(terminating: [])
    }

    @MainActor
    private func probeIconMenu() async {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let icon = board.icons["Settings"].firstMatch
        guard waitFor(icon, "the Settings icon", 5) else {
            capture(board, Self.springboardId, "no Settings icon found")
            return
        }
        let pressStart = Date()
        tolerant("long-press the Settings icon") { icon.press(forDuration: 1.2) }
        note(String(format: "the long press returned after %.1f s (XCUITest waits for SpringBoard to idle after it)", Date().timeIntervalSince(pressStart)))
        await pause(1)
        let menu = capture(board, Self.springboardId, "Settings icon menu up")
        report(menu, "menu-like nodes (raw type or identifier contains menu or callout)", Self.menuish)
        report(menu, "nodes of raw type Menu, MenuItem, Popover, Sheet or Other with an identifier", limit: 20) { e in
            [XCUIElementTypeCode.menu, XCUIElementTypeCode.menuItem, XCUIElementTypeCode.popover,
             XCUIElementTypeCode.sheet].contains(e.elementType)
                || (e.elementType == XCUIElementTypeCode.other && !e.identifier.isEmpty)
        }
        report(menu, "nodes labelled like icon menu items", limit: 20) { e in
            ["Edit Home Screen", "Remove App", "Share App", "Require Face ID", "Hide and Require Face ID", "Search", "Copy"].contains(e.label)
                || e.label.localizedCaseInsensitiveContains("Home Screen")
        }
        tolerant("press Home to dismiss the menu") { XCUIDevice.shared.press(.home) }
        await pause(1)
        let done = board.buttons["Done"].firstMatch
        if done.exists {
            note("a Done button is in SpringBoard's tree after dismissing (icon edit mode?); pressing Home again")
            tolerant("press Home again") { XCUIDevice.shared.press(.home) }
            await pause(1)
        }
    }

    // MARK: - (e) Safari's address control in landscape

    @MainActor
    func testFactE_SafariAddressInLandscape() async throws {
        executionTimeAllowance = 240
        begin("fact-e-safari-landscape")
        await probeSafariLandscape()
        await finish(terminating: [Self.safariId])
    }

    @MainActor
    private func probeSafariLandscape() async {
        defer { if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait } }
        let server: FormCaptureServer
        do {
            server = try FormCaptureServer()
        } catch {
            note("the local test server did not start: \(error)")
            return
        }
        defer { server.stop() }
        let url = server.baseURL.absoluteString + "/"
        guard let safari = await openInSafari(url, waitFor: "Contact form") else { return }
        note("extra Safari tabs (left by earlier tests) are not closed: Safari's tab overview offers no stable identifiers to close them by, so the open tabs are counted from the tab bar in landscape instead")
        reportAddress(capture(safari, Self.safariId, "Safari portrait, page loaded"), "portrait")
        tolerant("rotate to landscape left") { XCUIDevice.shared.orientation = .landscapeLeft }
        await pause(2.5)
        note("device orientation now \(XCUIDevice.shared.orientation.rawValue) (landscapeLeft is 3)")
        let landscape = capture(safari, Self.safariId, "Safari landscape left")
        reportAddress(landscape, "landscape")
        let tabs = reportAddressTabs(landscape, "landscape")
        tolerant("rotate back to portrait") { XCUIDevice.shared.orientation = .portrait }
        await pause(2)
        reportAddress(capture(safari, Self.safariId, "Safari portrait again"), "portrait again")
        note(tabs > 1
             ? "\(tabs) tabs are open (TabBarTab nodes in landscape): the inactive tabs' address capsules are in the landscape lines above, each with its tab's isActive flag"
             : "\(tabs) TabBarTab node(s) in landscape: no neighbouring tab's address capsule was available to record")
    }

    /// The tab bar's tabs (`TabBarTab?isActive=…` nodes), and every node labelled Address
    /// with the tab it sits in (the nearest node, itself included, whose identifier carries
    /// `isActive=`), so active and inactive tabs can be told apart. Returns the tab count.
    @MainActor
    @discardableResult
    private func reportAddressTabs(_ tree: RawElement?, _ what: String) -> Int {
        guard let tree else {
            note("\(what): no tree (the capture failed)")
            return 0
        }
        let tabs = Self.hits(tree, { $0.identifier.hasPrefix("TabBarTab") })
        note("\(what): TabBarTab nodes (the tabs in the tab bar): \(tabs.count)")
        for tab in tabs {
            let e = tab.element
            lines.append("        - " + Self.activeFlag(e.identifier) + " value \"\(Self.clip(e.value ?? ""))\" frame " + Self.rectText(e.frame) + (e.isSelected ? " selected" : ""))
        }
        let addresses = Self.hits(tree, { $0.label == "Address" })
        note("\(what): nodes labelled Address, with the tab they sit in: \(addresses.count)")
        for hit in addresses {
            let e = hit.element
            let chain: [RawElement] = [e] + Array(hit.ancestors.reversed())
            let owner = chain.first(where: { $0.identifier.contains("isActive=") })
            var line = "        - \(XCUIElementTypeCode.name(e.elementType)) value \"\(Self.clip(e.value ?? ""))\" id \(Self.clip(e.identifier)) frame " + Self.rectText(e.frame)
            if let owner {
                let kind = String(owner.identifier.prefix(while: { $0 != "?" }))
                line += "; in \(kind) " + Self.activeFlag(owner.identifier)
            } else {
                line += "; no node with isActive= around it"
            }
            lines.append(line)
        }
        return tabs.count
    }

    /// `isActive=true` or `isActive=false` out of a Safari tab identifier.
    private static func activeFlag(_ identifier: String) -> String {
        guard let range = identifier.range(of: "isActive=") else { return "isActive unknown" }
        let value = identifier[range.upperBound...].prefix(while: { $0 != "&" })
        return "isActive=\(value)"
    }

    @MainActor
    private func reportAddress(_ tree: RawElement?, _ what: String) {
        report(tree, "\(what): nodes naming the host 127.0.0.1", Self.mentions("127.0.0.1"))
        report(tree, "\(what): nodes labelled Address or with URL, Address or TabBarItemTitle in the id") { e in
            e.label == "Address" || e.identifier.localizedCaseInsensitiveContains("url")
                || e.identifier.localizedCaseInsensitiveContains("address") || e.identifier == "TabBarItemTitle"
        }
        report(tree, "\(what): TextField nodes") { $0.elementType == XCUIElementTypeCode.textField }
        report(tree, "\(what): Toolbar, TabBar and NavigationBar nodes") { e in
            [XCUIElementTypeCode.toolbar, XCUIElementTypeCode.tabBar, XCUIElementTypeCode.navigationBar].contains(e.elementType)
        }
    }

    // MARK: - (R23) Does a snapshot run the app's own accessibility code?

    private struct Reading {
        var kind: String
        var after: String
        var at: Double
        var label: String
        var count: Int?
    }

    @MainActor
    func testFactR23_SnapshotRunsAppAccessibilityCode() async throws {
        begin("fact-r23-snapshot-counter")
        await probeSnapshotCounter()
        await finish(terminating: [])
    }

    @MainActor
    private func probeSnapshotCounter() async {
        let app = launchPlayground()
        guard await openProbeScreen(app, "probe.counter") else {
            capture(app, Self.appBundleId, "the Snapshot counter screen was not reached")
            return
        }
        let counter = app.staticTexts["snapshot.counter"].firstMatch
        guard waitFor(counter, "the counter text snapshot.counter", 5) else {
            capture(app, Self.appBundleId, "no counter text")
            return
        }
        note("the counted element is a UIView whose accessibilityLabel getter counts its calls; the count is shown in a separate text (snapshot.counter, \"calls: N\") read with one plain XCUITest query (`.label`), itself an accessibility request")
        var readings: [Reading] = []
        readCounter(counter, "start", "the screen opened (navigation and waitForExistence)", &readings)
        readCounter(counter, "read", "one plain read", &readings)
        readCounter(counter, "read", "one plain read", &readings)
        await pause(5)
        readCounter(counter, "idle", "5 s with no query, then one plain read", &readings)

        let runner = XCUITestPhoneRunner()
        runner.noteActivated(Self.appBundleId)
        runner.onObservation = { [weak self] record in self?.attachObservation(record) }
        for i in 1...3 {
            do {
                _ = try await runner.snapshot(includeScreenshot: false)
            } catch {
                note("runner snapshot \(i) threw: \(error)")
            }
            await pause(1)
            readCounter(counter, "snapshot", "runner snapshot \(i) (PhoneRunner.snapshot: app.snapshot() plus the screenshot it always takes), 1 s, one plain read", &readings)
        }
        runner.onObservation = nil
        for i in 1...3 {
            _ = XCUIScreen.main.screenshot()
            await pause(1)
            readCounter(counter, "screenshot", "XCUIScreen.main.screenshot() \(i), 1 s, one plain read", &readings)
        }
        readCounter(counter, "read", "one plain read", &readings)
        readCounter(counter, "read", "one plain read", &readings)
        capture(app, Self.appBundleId, "counter screen (this probe's capture: app.snapshot() and a screenshot)")
        readCounter(counter, "capture", "the probe's own capture, one plain read", &readings)

        // Round 2 additions, after the unchanged sequence above (so the two runs compare).
        tolerant("app.snapshot() alone") { _ = try app.snapshot() }
        await pause(1)
        readCounter(counter, "snapshot-only", "app.snapshot() alone (no screenshot), 1 s, one plain read", &readings)
        let counted = app.descendants(matching: .any)["snapshot.counted"].firstMatch
        var countedExists = false
        tolerant("resolve the counted element by its identifier") { countedExists = counted.exists }
        await pause(1)
        readCounter(counter, "query-exists", "a query resolving the counted element by its identifier (.exists), 1 s, one plain read", &readings)
        var countedLabel = "(unread)"
        tolerant("read the counted element's label by query") { countedLabel = counted.label }
        await pause(1)
        readCounter(counter, "query-label", "the counted element's .label by query, 1 s, one plain read", &readings)
        var described = 0
        tolerant("read app.debugDescription") { described = app.debugDescription.count }
        await pause(1)
        readCounter(counter, "debugDescription", "app.debugDescription, 1 s, one plain read", &readings)
        note("the counted element by query: exists \(countedExists), label \"\(countedLabel)\"; app.debugDescription was \(described) characters")

        lines.append("readings (seconds since the probe started, the text read, the change since the reading before, what happened in between):")
        var byKind: [String: [Int]] = [:]
        for (i, r) in readings.enumerated() {
            var delta: Int?
            if i > 0, let now = r.count, let before = readings[i - 1].count { delta = now - before }
            if let delta { byKind[r.kind, default: []].append(delta) }
            lines.append(String(format: "    %7.2f s  ", r.at) + "\"\(r.label)\"  change \(delta.map { String($0) } ?? "-")  after: \(r.after)")
        }
        for kind in ["read", "idle", "snapshot", "screenshot", "capture", "snapshot-only", "query-exists", "query-label", "debugDescription"] {
            let deltas = byKind[kind] ?? []
            note("changes after \(kind): \(deltas.isEmpty ? "none readable" : deltas.map { String($0) }.joined(separator: ", "))")
        }
        note("how to read the changes: the shown count is updated on the main queue after each counted call, so a reading shows the calls up to just before (and possibly during) it; every change includes the plain read(s) themselves, so a step's own share is its change minus the change after a plain read. A step that moves the count by more than a plain read ran the app's accessibility code; a step that moves it by no more than a plain read did not")
    }

    @MainActor
    private func readCounter(_ element: XCUIElement, _ kind: String, _ after: String, _ readings: inout [Reading]) {
        var label = "(unreadable)"
        tolerant("read the counter") { label = element.label }
        let count = label.split(separator: " ").last.flatMap { Int(String($0)) }
        readings.append(Reading(kind: kind, after: after, at: Date().timeIntervalSince(started), label: label, count: count))
    }

    // MARK: - (i) Can a web page detect a snapshot?

    @MainActor
    func testFactI_WebPageSeesSnapshots() async throws {
        executionTimeAllowance = 240
        begin("fact-i-web-snapshot")
        await probeWebPage()
        await finish(terminating: [Self.safariId])
    }

    @MainActor
    private func probeWebPage() async {
        let server: FormCaptureServer
        do {
            // The server serves its page for every GET, so the probe page needs no new route.
            server = try FormCaptureServer(formHTML: Self.probePage)
        } catch {
            note("the local test server did not start: \(error)")
            return
        }
        defer { server.stop() }
        let url = server.baseURL.absoluteString + "/"
        guard let safari = await openInSafari(url, waitFor: "Snapshot probe") else { return }
        let out = safari.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "probe t=")).firstMatch
        guard waitFor(out, "the page's counter line", 10) else {
            capture(safari, Self.safariId, "no counter line on the page")
            return
        }
        note("the page logs focus, blur, focusin, focusout, visibilitychange, scroll, selectionchange, resize, pointerdown and touchstart events, document.hasFocus() and document.visibilityState (and how often each changed), and runs a 10 ms heartbeat (h) and a requestAnimationFrame loop (r); every gap over 50 ms is kept (the last \(Self.gapRing) of each kind) as kind:Date.now():performance.now():gap ms. Its line is re-rendered every 500 ms and read with one plain XCUITest query, only before and after the snapshots. The simulator and this test share the Mac's clock, so Date.now() on the page and the probe's wall clock line up")
        await pause(2)
        var readings: [Reading] = []
        let baseStart = Self.epochMs()
        await pause(10)
        let baseEnd = Self.epochMs()
        readCounter(out, "baseline", "10 s quiet (the baseline window), one plain read", &readings)
        let runner = XCUITestPhoneRunner()
        runner.noteActivated(Self.safariId)
        var windows: [(start: Double, end: Double)] = []
        for i in 1...20 {
            let start = Self.epochMs()
            do {
                _ = try await runner.snapshot(includeScreenshot: false)
            } catch {
                note("runner snapshot \(i) threw: \(error)")
            }
            windows.append((start: start, end: Self.epochMs()))
            await pause(0.5)
        }
        await pause(1)
        readCounter(out, "snapshot", "20 runner snapshots 500 ms apart, 1 s, one plain read", &readings)
        await pause(3)
        readCounter(out, "idle", "3 s quiet, one plain read", &readings)
        lines.append("page readings (seconds since the probe started, then the page's own line):")
        for r in readings {
            lines.append(String(format: "    %7.2f s  ", r.at) + "after \(r.after): \"\(r.label)\"")
        }
        analyseWebGaps(readings, baseStart: baseStart, baseEnd: baseEnd, windows: windows)
        let log = server.log
        note("server log: " + (log.isEmpty ? "(no request reached the server)" : log.joined(separator: " | ")))
    }

    /// A gap the page kept: `kind` h (heartbeat) or r (animation frame), its end on the shared
    /// wall clock (`Date.now()`, ms) and in page time (`performance.now()`), and its length.
    private struct PageGap: Equatable {
        var kind: String
        var epoch: Double
        var pageTime: Double
        var gap: Double
    }

    /// Gaps kept per kind by the page (its `keep` ring).
    private static let gapRing = 60

    private static func epochMs() -> Double {
        Date().timeIntervalSince1970 * 1_000
    }

    /// The value of `name=` in the page's line.
    private static func pageField(_ line: String, _ name: String) -> String? {
        for token in line.split(separator: " ") where token.hasPrefix(name + "=") {
            return String(token.dropFirst(name.count + 1))
        }
        return nil
    }

    /// The gaps listed in the page's line (`hgaps=` and `rgaps=`).
    private static func pageGaps(_ line: String) -> [PageGap] {
        var out: [PageGap] = []
        for name in ["hgaps", "rgaps"] {
            guard let list = pageField(line, name) else { continue }
            for item in list.split(separator: ",") {
                let parts = item.split(separator: ":").map { String($0) }
                guard parts.count == 4, let epoch = Double(parts[1]), let pageTime = Double(parts[2]),
                      let gap = Double(parts[3]) else { continue }
                out.append(PageGap(kind: parts[0], epoch: epoch, pageTime: pageTime, gap: gap))
            }
        }
        return out
    }

    /// Lines the 20 snapshot windows up with the page's gaps over 50 ms, against the rate of
    /// such gaps in the quiet baseline window, and says whether any focus, visibility or event
    /// counter changed.
    @MainActor
    private func analyseWebGaps(_ readings: [Reading], baseStart: Double, baseEnd: Double, windows: [(start: Double, end: Double)]) {
        guard let baseline = readings.first, Self.pageField(baseline.label, "off") != nil else {
            note("the page's line could not be read (no off= field); no gap analysis")
            return
        }
        let offset = Double(Self.pageField(baseline.label, "off") ?? "") ?? 0
        var gaps: [PageGap] = []
        for r in readings {
            for g in Self.pageGaps(r.label) where !gaps.contains(g) { gaps.append(g) }
        }
        gaps.sort { $0.epoch < $1.epoch }
        // Where the baseline reading's rings start: a full ring has lost its oldest gaps.
        let baseGapsRead = Self.pageGaps(baseline.label)
        var covered = baseStart
        for kind in ["h", "r"] {
            let ofKind = baseGapsRead.filter { $0.kind == kind }
            if ofKind.count >= Self.gapRing, let oldest = ofKind.first?.epoch { covered = max(covered, oldest) }
        }
        let span = max(1, baseEnd - covered)
        let inBase = gaps.filter { $0.epoch >= covered && $0.epoch <= baseEnd }
        let rate = Double(inBase.count) / span
        let totalGap: Double = inBase.map { $0.gap }.reduce(0, +)
        let meanGap: Double = inBase.isEmpty ? 50 : totalGap / Double(inBase.count)
        let heartbeatCount = inBase.filter { $0.kind == "h" }.count
        let frameCount = inBase.filter { $0.kind == "r" }.count
        let coverage = covered > baseStart ? "the rings had already dropped the window's start" : "the whole window"
        note("baseline: \(inBase.count) gap(s) over 50 ms (heartbeat \(heartbeatCount), animation frame \(frameCount)) in "
             + String(format: "%.1f s", span / 1_000) + " of quiet (\(coverage)), "
             + String(format: "%.2f per second, mean length %.0f ms; page time = wall clock - %.0f ms", rate * 1_000, meanGap, offset))

        lines.append("snapshot windows (wall clock ms, page time ms = wall clock - offset, the call's length, gaps over 50 ms overlapping the window widened by 60 ms on each side):")
        var hits = 0
        var expected = 0.0
        for (i, w) in windows.enumerated() {
            let low = w.start - 60
            let high = w.end + 60
            let near = gaps.filter { g in g.epoch >= low && g.epoch - g.gap <= high }
            if !near.isEmpty { hits += 1 }
            expected += min(1, rate * (high - low + meanGap))
            let listed = near.map { g in g.kind + String(format: " %.0f ms ending at page %.0f", g.gap, g.pageTime) }
            lines.append("    \(i + 1)  " + String(format: "wall %.0f  page %.0f  took %.0f ms  ", w.start, w.start - offset, w.end - w.start)
                         + (listed.isEmpty ? "no gap" : listed.joined(separator: "; ")))
        }
        if let first = windows.first {
            let snapshotRead = readings.first(where: { $0.kind == "snapshot" }).map { Self.pageGaps($0.label) } ?? []
            for kind in ["h", "r"] {
                let ofKind = snapshotRead.filter { $0.kind == kind }
                if ofKind.count >= Self.gapRing, let oldest = ofKind.first?.epoch, oldest > first.start {
                    note("the page's \(kind) ring had dropped gaps from before \(String(format: "%.0f", oldest)) when it was read after the snapshots: the earliest snapshot windows may have lost gaps")
                }
            }
        }
        note("RESULT: \(hits) of \(windows.count) snapshot times fall within ±60 ms of a heartbeat or animation-frame gap over 50 ms; by chance, from the baseline rate, about "
             + String(format: "%.1f", expected) + " would")

        // Focus, visibility and event counters, against the baseline reading.
        let fields = ["hasFocus", "vis", "hfChanges", "visChanges", "focus", "blur", "focusin", "focusout", "visibilitychange",
                      "scroll", "selectionchange", "resize", "pointerdown", "touchstart"]
        for r in readings.dropFirst() {
            let changed = fields.filter { Self.pageField(r.label, $0) != Self.pageField(baseline.label, $0) }
            let described = changed.map { "\($0) \(Self.pageField(baseline.label, $0) ?? "?") -> \(Self.pageField(r.label, $0) ?? "?")" }
            note("against the baseline, after \(r.after): " + (changed.isEmpty ? "document.hasFocus(), document.visibilityState and every event counter unchanged" : "changed: " + described.joined(separator: ", ")))
        }
    }

    /// The probe page for (i): event counters, focus and visibility, a 10 ms heartbeat and an
    /// animation-frame loop with their gaps over 50 ms, shown as one line of text.
    private static let probePage = """
    <!doctype html>
    <html><head><meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Jarvis QA probe</title>
    <style>
    body { font: 18px -apple-system, sans-serif; margin: 20px; }
    p { word-break: break-all; font-size: 7px; }
    </style></head>
    <body>
    <h1>Snapshot probe</h1>
    <p id="out">probe t=0 starting</p>
    <script>
    var counts = { focus: 0, blur: 0, focusin: 0, focusout: 0, visibilitychange: 0, scroll: 0, selectionchange: 0, resize: 0, pointerdown: 0, touchstart: 0 };
    function bump(name) { return function () { counts[name] += 1; }; }
    ['focus', 'blur', 'scroll', 'resize'].forEach(function (n) { window.addEventListener(n, bump(n), true); });
    ['focusin', 'focusout', 'visibilitychange', 'selectionchange', 'pointerdown', 'touchstart'].forEach(function (n) { document.addEventListener(n, bump(n), true); });
    var beats = 0, frames = 0, hMax = 0, rMax = 0, h50 = 0, r50 = 0, hGaps = [], rGaps = [];
    var hLast = performance.now(), rLast = performance.now();
    var lastFocus = document.hasFocus(), lastVis = document.visibilityState, hfChanges = 0, visChanges = 0;
    function keep(list, kind, now, gap) {
      list.push(kind + ':' + Date.now() + ':' + Math.round(now) + ':' + Math.round(gap));
      if (list.length > 60) { list.shift(); }
    }
    setInterval(function () {
      var now = performance.now(), gap = now - hLast;
      hLast = now;
      beats += 1;
      if (gap > hMax) { hMax = gap; }
      if (gap > 50) { h50 += 1; keep(hGaps, 'h', now, gap); }
      var f = document.hasFocus(), v = document.visibilityState;
      if (f !== lastFocus) { hfChanges += 1; lastFocus = f; }
      if (v !== lastVis) { visChanges += 1; lastVis = v; }
    }, 10);
    function frame() {
      var now = performance.now(), gap = now - rLast;
      rLast = now;
      frames += 1;
      if (gap > rMax) { rMax = gap; }
      if (gap > 50) { r50 += 1; keep(rGaps, 'r', now, gap); }
      requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);
    function render() {
      var t = performance.now();
      var parts = ['probe t=' + Math.round(t), 'off=' + Math.round(Date.now() - t), 'beats=' + beats, 'frames=' + frames,
        'hmax=' + Math.round(hMax), 'rmax=' + Math.round(rMax), 'h50=' + h50, 'r50=' + r50,
        'hasFocus=' + (document.hasFocus() ? 1 : 0), 'vis=' + document.visibilityState, 'hfChanges=' + hfChanges, 'visChanges=' + visChanges];
      Object.keys(counts).forEach(function (k) { parts.push(k + '=' + counts[k]); });
      parts.push('hgaps=' + hGaps.join(','));
      parts.push('rgaps=' + rGaps.join(','));
      document.getElementById('out').textContent = parts.join(' ');
    }
    setInterval(render, 500);
    render();
    </script>
    </body></html>
    """

    // MARK: - Driving helpers

    /// Opens `url` in Safari from the Home Screen (as the Safari agent test starts) and waits
    /// for a static text `heading`; closes Safari's first-launch tip if it is up. Returns
    /// Safari, or nil (recorded) when Safari did not come to the front.
    @MainActor
    private func openInSafari(_ url: String, waitFor heading: String) async -> XCUIApplication? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [Self.safariId])
        let safari = XCUIApplication(bundleIdentifier: Self.safariId)
        guard let target = URL(string: url) else {
            note("not a URL: \(url)")
            return nil
        }
        note("opening \(url) in Safari")
        tolerant("open the URL") { XCUIDevice.shared.system.open(target) }
        guard safari.wait(for: .runningForeground, timeout: 15) else {
            note("Safari did not reach the foreground in 15 s (state \(safari.state.rawValue))")
            capture(XCUIApplication(bundleIdentifier: Self.springboardId), Self.springboardId, "Safari not in front")
            return nil
        }
        let text = Self.node(in: safari, labelled: heading)
        note(waitFor(text, "the page heading \"\(heading)\"", 30)
             ? "the page heading \"\(heading)\" is in Safari's tree"
             : "the page heading \"\(heading)\" did not appear in Safari's tree within 30 s")
        let tip = safari.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "You can now view these items")).firstMatch
        if tip.exists {
            capture(safari, Self.safariId, "Safari's first-launch tip, before closing it")
            tolerant("close Safari's tip") {
                let close = safari.buttons["Close"].firstMatch
                if close.exists { close.tap() } else { note("the tip has no Close button") }
            }
            await pause(1)
        }
        return safari
    }

    /// Opens the Playground's "Fact probes" row, then the probe screen `id`.
    @MainActor
    private func openProbeScreen(_ app: XCUIApplication, _ id: String) async -> Bool {
        let entry = app.buttons["playground.probes"].firstMatch
        guard waitFor(entry, "the Playground's Fact probes row", 10) else { return false }
        tolerant("open Fact probes") { entry.tap() }
        let row = app.buttons[id].firstMatch
        guard waitFor(row, "the \(id) row", 5) else { return false }
        tolerant("open \(id)") { row.tap() }
        await pause(1)
        return true
    }

    /// A vertical drag on `app` between two normalised heights at mid-width.
    @MainActor
    private func drag(_ app: XCUIApplication, from: Double, to: Double, _ what: String) {
        tolerant(what) {
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: from))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: to))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        note("\(what): dragged from \(Int(from * 100))% to \(Int(to * 100))% of the height")
    }

    /// The first running app of `candidates` whose tree has a keyboard.
    @MainActor
    private func keyboardOwner(_ candidates: [(String, XCUIApplication)]) -> (String, XCUIApplication)? {
        for (id, app) in candidates where isRunning(app) {
            var has = false
            tolerant("look for a keyboard in \(id)") { has = app.keyboards.firstMatch.exists }
            if has { return (id, app) }
        }
        return nil
    }

    /// Whether `app` runs (read tolerantly: Spotlight's id is not certain on every iOS).
    @MainActor
    private func isRunning(_ app: XCUIApplication) -> Bool {
        var running = false
        tolerant("read an app state") {
            let state = app.state
            running = state == .runningForeground || state == .runningBackground
        }
        return running
    }

    @MainActor
    private func stateText(_ app: XCUIApplication) -> String {
        var text = "?"
        tolerant("read an app state") { text = "\(app.state.rawValue)" }
        return text
    }

    /// The first node of any type whose label is exactly `text` (a web heading need not be a
    /// StaticText).
    @MainActor
    private static func node(in app: XCUIApplication, labelled text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", text)).firstMatch
    }

    @MainActor
    private func waitFor(_ element: XCUIElement, _ what: String, _ timeout: TimeInterval) -> Bool {
        var found = false
        tolerant("wait for \(what)") { found = element.waitForExistence(timeout: timeout) }
        if !found { note("\(what): not found within \(timeout.compactText) s") }
        return found
    }

    /// The app in front by the runner's own reading (`FrontAppGuard`'s).
    @MainActor
    private func frontApp() async -> String {
        let runner = XCUITestPhoneRunner()
        let front: String? = try? await runner.frontBundleId()
        return front ?? "?"
    }

    /// What Jarvis's own runner sees at this moment: the code the agent runs
    /// (`XCUITestPhoneRunner().frontBundleId()` and `.snapshot(includeScreenshot: false)`, with no
    /// app noted as activated), its snapshot attached like a capture and checked for result
    /// rows, search and text fields, web views and (with `query`) nodes mentioning it.
    @MainActor
    private func runnerView(_ moment: String, query: String? = nil) async {
        let runner = XCUITestPhoneRunner()
        runnerRecord = nil
        runner.onObservation = { [weak self] record in self?.keepRunnerRecord(record) }
        var front = "?"
        do {
            let id = try await runner.frontBundleId()
            front = id ?? "nil"
        } catch {
            front = "threw \(error)"
        }
        note("\(moment): the runner's frontBundleId() is \(front)")
        do {
            let observation = try await runner.snapshot(includeScreenshot: false)
            note("\(moment): the runner's snapshot reports app \(observation.appBundleId); notes: " + (observation.notes.isEmpty ? "none" : observation.notes.joined(separator: " | ")))
        } catch {
            note("\(moment): the runner's snapshot threw \(error)")
        }
        runner.onObservation = nil
        guard let record = runnerRecord else {
            note("\(moment): the runner reported no observation")
            return
        }
        runnerRecord = nil
        note("\(moment): the runner's snapshot is of \(record.bundleId), \(record.built.nodeCount) nodes (root \"\(Self.clip(record.raw.label))\")")
        let tree: RawElement? = record.raw
        report(tree, "\(moment), the runner's snapshot: Cell nodes (result rows)", limit: 8) { $0.elementType == XCUIElementTypeCode.cell }
        report(tree, "\(moment), the runner's snapshot: SearchField and TextField nodes") { e in
            e.elementType == XCUIElementTypeCode.searchField || e.elementType == XCUIElementTypeCode.textField
        }
        report(tree, "\(moment), the runner's snapshot: WebView nodes") { $0.elementType == XCUIElementTypeCode.webView }
        if let query {
            report(tree, "\(moment), the runner's snapshot: nodes mentioning \"\(query)\"", limit: 8, Self.mentions(query))
        }
    }

    @MainActor
    private func keepRunnerRecord(_ record: RunnerObservationRecord) {
        runnerRecord = record
        attachObservation(record)
    }

    /// The frame XCUITest's query reports for `element` (resolving it), nil when it does not
    /// exist or the read fails.
    @MainActor
    private func queryFrame(_ element: XCUIElement) -> CGRect? {
        var frame: CGRect?
        tolerant("read a frame by query") {
            if element.exists { frame = element.frame }
        }
        return frame
    }

    private static func frameText(_ r: CGRect) -> String {
        String(format: "{%.0f,%.0f %.0fx%.0f}", Double(r.origin.x), Double(r.origin.y), Double(r.width), Double(r.height))
    }

    private static func rectText(_ f: Rect) -> String {
        String(format: "{%.0f,%.0f %.0fx%.0f}", f.x, f.y, f.width, f.height)
    }

    /// A cheap checksum of a screenshot's bytes (for the record; equality is checked on the data).
    private static func checksum(_ data: Data) -> UInt32 {
        var sum: UInt32 = 0
        for byte in data { sum = sum &* 31 &+ UInt32(byte) }
        return sum
    }

    private func pause(_ seconds: Double) async {
        try? await Task.sleep(for: .milliseconds(Int(seconds * 1_000)))
    }

    /// Runs one probing action; XCTest issues it raises are captured as data (the lock
    /// probe's pattern). Returns whether it ran without an issue or a throw.
    @MainActor
    @discardableResult
    private func tolerant(_ what: String, _ body: () throws -> Void) -> Bool {
        var issues: [String] = []
        let options = XCTExpectedFailure.Options()
        options.isStrict = false
        options.issueMatcher = { issue in
            issues.append(issue.compactDescription)
            return true
        }
        var ok = true
        do {
            _ = try XCTExpectFailure("fact probe: \(what) may fail; recorded, not judged", options: options) { () throws -> Bool in
                try body()
                return true
            }
        } catch {
            ok = false
            note("\(what): threw \(error)")
        }
        if !issues.isEmpty {
            ok = false
            note("\(what): XCTest issues (recorded, not failures): " + issues.joined(separator: " | "))
        }
        return ok
    }

    // MARK: - Recording

    @MainActor
    private func begin(_ name: String) {
        // Probes record; an XCTest issue must not stop the rest of the probe.
        continueAfterFailure = true
        fact = name
        captures = 0
        lines = []
        started = Date()
        note("probe \(name) started (\(ProcessInfo.processInfo.operatingSystemVersionString))")
    }

    @MainActor
    private func note(_ text: String) {
        lines.append(String(format: "t+%6.1f s  ", Date().timeIntervalSince(started)) + text)
    }

    /// Leaves the simulator as the other tests expect it (portrait, Home Screen, Jarvis and
    /// the system apps used terminated) and writes `<fact>.result.txt`.
    @MainActor
    private func finish(terminating ids: [String]) async {
        if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
        let own = XCUIApplication()
        if own.state != .notRunning {
            tolerant("terminate Jarvis") { own.terminate() }
        }
        await SystemAppAgentTests.startOnHomeScreen(terminating: ids)
        note("probe ended: Home Screen, portrait")
        let text = "FACT PROBE \(fact)\n" + lines.joined(separator: "\n") + "\n"
        attach(XCTAttachment(string: text), "\(fact).result.txt")
        print(text)
    }

    /// Captures `app`'s tree and a screenshot as `<fact>.NN-tree.txt` and `<fact>.NN.png`, with
    /// the harness's dump. Returns the raw copy, or nil (recorded) when the snapshot failed.
    @MainActor
    @discardableResult
    private func capture(_ app: XCUIApplication, _ bundleId: String, _ moment: String) -> RawElement? {
        captures += 1
        let n = String(format: "%02d", captures)
        let state = app.state
        let start = Date()
        var copied: RawElement?
        var failure = ""
        tolerant("snapshot of \(bundleId)") {
            do {
                let snapshot = try app.snapshot()
                copied = SnapshotCopy.copy(snapshot)
            } catch {
                failure = "\(error)"
            }
        }
        guard let raw = copied else {
            note("capture \(n) (\(moment)): \(bundleId) snapshot failed (state \(state.rawValue)): \(failure)")
            attach(XCTAttachment(string: "capture \(n): \(moment)\n\(bundleId), state \(state.rawValue): the snapshot failed: \(failure)\n"), "\(fact).\(n)-tree.txt")
            return nil
        }
        let ms = Int(Date().timeIntervalSince(start) * 1_000)
        let frame = raw.frame.sanitized
        let size: Size? = frame.width > 0 && frame.height > 0 ? Size(width: frame.width, height: frame.height) : nil
        let built = UiTreeBuilder.build(raw, screenSize: size)
        let shot = XCUIScreen.main.screenshot()
        let record = RunnerObservationRecord(
            number: captures, bundleId: bundleId, raw: raw, built: built, screenshot: shot,
            notes: ["fact probe \(fact), capture \(n): \(moment)",
                    "app state \(state.rawValue), device orientation \(XCUIDevice.shared.orientation.rawValue)"],
            snapshotMs: ms
        )
        attach(XCTAttachment(string: QARun.treeText(record)), "\(fact).\(n)-tree.txt")
        attach(XCTAttachment(screenshot: shot), "\(fact).\(n).png")
        note("capture \(n) (\(moment)): \(bundleId), state \(state.rawValue), \(built.nodeCount) nodes, snapshot \(ms) ms -> \(fact).\(n)-tree.txt")
        return raw
    }

    /// Attaches `app.debugDescription` (XCUITest's query view of the app, which reaches remote
    /// content the snapshot does not) as `<fact>.NN-query.txt`.
    @MainActor
    private func attachQuery(_ app: XCUIApplication, _ moment: String) {
        captures += 1
        let n = String(format: "%02d", captures)
        let start = Date()
        var text = "(not read)"
        tolerant("read debugDescription") { text = app.debugDescription }
        let ms = Int(Date().timeIntervalSince(start) * 1_000)
        attach(XCTAttachment(string: "capture \(n): \(moment)\nXCUIApplication.debugDescription (the query view), read in \(ms) ms\n\n" + text), "\(fact).\(n)-query.txt")
        note("capture \(n) (\(moment)): debugDescription, \(text.count) characters, \(ms) ms -> \(fact).\(n)-query.txt")
    }

    /// An observation the runner made itself (R23, (i)), attached like a capture.
    @MainActor
    private func attachObservation(_ record: RunnerObservationRecord) {
        captures += 1
        let n = String(format: "%02d", captures)
        attach(XCTAttachment(string: QARun.treeText(record)), "\(fact).\(n)-tree.txt")
        if let shot = record.screenshot { attach(XCTAttachment(screenshot: shot), "\(fact).\(n).png") }
        note("capture \(n): runner observation \(record.number) of \(record.bundleId), \(record.built.nodeCount) nodes, snapshot \(record.snapshotMs) ms -> \(fact).\(n)-tree.txt")
    }

    @MainActor
    private func attach(_ attachment: XCTAttachment, _ name: String) {
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    // MARK: - Simple checks on a raw tree

    private struct Hit {
        var element: RawElement
        var ancestors: [RawElement]
    }

    /// Writes how many nodes of `tree` match, and up to `limit` of them described.
    @MainActor
    private func report(_ tree: RawElement?, _ what: String, limit: Int = 10, _ match: (RawElement) -> Bool) {
        guard let tree else {
            note("\(what): no tree (the capture failed)")
            return
        }
        let found = Self.hits(tree, match)
        note("\(what): \(found.count)")
        for hit in found.prefix(limit) {
            lines.append("        - " + Self.describe(hit, screen: tree.frame))
        }
        if found.count > limit { lines.append("        - and \(found.count - limit) more (see the tree file)") }
    }

    private static func hits(_ root: RawElement, _ match: (RawElement) -> Bool) -> [Hit] {
        var out: [Hit] = []
        var stack: [Hit] = [Hit(element: root, ancestors: [])]
        while let item = stack.popLast() {
            if match(item.element) { out.append(item) }
            let path = item.ancestors + [item.element]
            for child in item.element.children.reversed() {
                stack.append(Hit(element: child, ancestors: path))
            }
        }
        return out
    }

    /// `Button(9) role Button label "AirDrop" frame {16,700 80x90} on screen yes; inside Sheet
    /// yes; menu/callout ancestor no; ancestors: Application > Window > …`.
    private static func describe(_ hit: Hit, screen: Rect) -> String {
        let e = hit.element
        var s = "\(XCUIElementTypeCode.name(e.elementType))(\(e.elementType)) role \(UiTreeBuilder.roleName(XCUIElementTypeCode.role(e.elementType)))"
        if !e.label.isEmpty { s += " label \"\(clip(e.label))\"" }
        if !e.identifier.isEmpty { s += " id \(clip(e.identifier))" }
        if let value = e.value, !value.isEmpty { s += " value \"\(clip(value))\"" }
        if !e.title.isEmpty { s += " title \"\(clip(e.title))\"" }
        let f = e.frame
        s += String(format: " frame {%.0f,%.0f %.0fx%.0f}", f.x, f.y, f.width, f.height)
        s += " on screen " + (onScreen(f, screen) ? "yes" : "no")
        if !e.isEnabled { s += " disabled" }
        if e.isSelected { s += " selected" }
        let sheet = hit.ancestors.contains { $0.elementType == XCUIElementTypeCode.sheet }
        s += "; inside a raw Sheet " + (sheet ? "yes" : "no")
        if let menu = hit.ancestors.last(where: foreignMenuish) {
            s += "; menu/callout ancestor \(XCUIElementTypeCode.name(menu.elementType))" + (menu.identifier.isEmpty ? "" : " id \(clip(menu.identifier))")
        } else {
            s += "; menu/callout ancestor none"
        }
        s += "; ancestors: " + hit.ancestors.map { a in
            a.identifier.isEmpty ? XCUIElementTypeCode.name(a.elementType) : "\(XCUIElementTypeCode.name(a.elementType))#\(clip(a.identifier))"
        }.joined(separator: " > ")
        return s
    }

    private static func onScreen(_ f: Rect, _ screen: Rect) -> Bool {
        guard f.width > 0, f.height > 0, screen.width > 0, screen.height > 0 else { return false }
        return f.x < screen.x + screen.width && f.x + f.width > screen.x
            && f.y < screen.y + screen.height && f.y + f.height > screen.y
    }

    private static func clip(_ text: String) -> String {
        let flat = text.replacingOccurrences(of: "\n", with: " ")
        return flat.count > 140 ? String(flat.prefix(140)) + "…" : flat
    }

    /// A raw type or identifier that names a menu or a callout (the core's Rule P test).
    private static func menuish(_ e: RawElement) -> Bool {
        let type = XCUIElementTypeCode.name(e.elementType).lowercased()
        let id = e.identifier.lowercased()
        return type.contains("menu") || id.contains("menu") || id.contains("callout")
    }

    /// `menuish`, leaving out the probe fixtures' own `probe.*` identifiers and navigation bars
    /// (the "Menus" screen's title made them match in round 1).
    private static func foreignMenuish(_ e: RawElement) -> Bool {
        menuish(e) && !e.identifier.hasPrefix("probe.") && e.elementType != XCUIElementTypeCode.navigationBar
    }

    /// Label, value or identifier contains `text` (case-insensitive).
    private static func mentions(_ text: String) -> (RawElement) -> Bool {
        { e in
            e.label.localizedCaseInsensitiveContains(text) || (e.value ?? "").localizedCaseInsensitiveContains(text)
                || e.identifier.localizedCaseInsensitiveContains(text)
        }
    }
}
