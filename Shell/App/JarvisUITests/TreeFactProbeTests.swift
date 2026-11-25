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
/// capture, and one `<fact>.result.txt`. A probe never judges: a missing element is recorded
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
        let app = launchPlayground(extraArguments: ["-jarvis-probe-url", url])
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        guard await openProbeScreen(app, "probe.sheets") else {
            capture(app, Self.appBundleId, "the Browser sheets probe screen was not reached")
            return
        }

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
            var closed = false
            tolerant("tap Done (or Close)") {
                for label in ["Done", "Close"] where !closed {
                    let button = app.buttons[label].firstMatch
                    if button.exists {
                        button.tap()
                        closed = true
                        note("tapped \(label) to close the in-app browser")
                    }
                }
            }
            await pause(1.5)
            if !closed {
                note("no Done button in Jarvis's tree to close the in-app browser: relaunching Jarvis for the web sign-in")
                _ = launchPlayground(extraArguments: ["-jarvis-probe-url", url])
                guard await openProbeScreen(app, "probe.sheets") else {
                    capture(app, Self.appBundleId, "the Browser sheets probe screen was not reached after the relaunch")
                    return
                }
            }
        }

        // 2. ASWebAuthenticationSession: a consent alert first, then the sheet.
        let auth = app.buttons["probe.sheets.auth"].firstMatch
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
        if let (ownerId, owner) = alertOwner {
            note("the consent alert was found by query in \(ownerId)'s tree")
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
        tolerant("cancel the web sign-in") {
            let cancel = app.buttons["Cancel"].firstMatch
            if cancel.exists { cancel.tap() } else { note("no Cancel button in Jarvis's tree") }
        }
        await pause(1)
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

    // MARK: - (c) Edit menu and context menu

    @MainActor
    func testFactC_EditAndContextMenus() async throws {
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

        let selectAll = app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", "Select All")).firstMatch
        if !selectAll.exists {
            note("no \"Select All\" node after the double-tap; trying a long press on the field")
            tolerant("long-press the text field") { field.press(forDuration: 1.0) }
            await pause(1.2)
            let long = capture(app, Self.appBundleId, "after a long press on the text field")
            reportMenus(long, "edit menu after a long press, Jarvis's tree")
        }
        if selectAll.exists {
            tolerant("tap Select All") { selectAll.tap() }
            await pause(1.2)
            let all = capture(app, Self.appBundleId, "after Select All (edit menu for the whole text expected)")
            reportMenus(all, "edit menu after Select All, Jarvis's tree")
        } else {
            note("no \"Select All\" node found; not tapped")
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
        report(tree, "\(what): nodes whose raw type or identifier contains \"menu\" or \"callout\"", Self.menuish)
    }

    // MARK: - (g) Share sheet

    @MainActor
    func testFactG_ShareSheet() async throws {
        begin("fact-g-share-sheet")
        await probeShareSheet()
        await finish(terminating: [])
    }

    @MainActor
    private func probeShareSheet() async {
        let app = launchPlayground()
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        guard await openProbeScreen(app, "probe.share") else {
            capture(app, Self.appBundleId, "the Share sheet probe screen was not reached")
            return
        }
        let open = app.buttons["probe.share.open"].firstMatch
        guard waitFor(open, "the Share text button", 5) else {
            capture(app, Self.appBundleId, "no Share text button")
            return
        }
        tolerant("tap Share text") { open.tap() }
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
        reportShare(capture(app, Self.appBundleId, "share sheet as presented (Jarvis)"), "as presented, Jarvis's tree")
        reportShare(capture(board, Self.springboardId, "share sheet as presented (SpringBoard)"), "as presented, SpringBoard's tree")

        drag(app, from: 0.62, to: 0.12, "swipe the sheet up (expand)")
        await pause(1.5)
        reportShare(capture(app, Self.appBundleId, "after swiping the sheet up (Jarvis)"), "after the swipe up, Jarvis's tree")

        drag(app, from: 0.85, to: 0.2, "scroll the sheet's list up (top row off screen)")
        await pause(1.5)
        reportShare(capture(app, Self.appBundleId, "after scrolling the list (Jarvis)"), "after scrolling, Jarvis's tree")
        reportShare(capture(board, Self.springboardId, "after scrolling the list (SpringBoard)"), "after scrolling, SpringBoard's tree")

        tolerant("close the share sheet") {
            let close = app.buttons["Close"].firstMatch
            if close.exists { close.tap() } else { note("no Close button in Jarvis's tree; the app is terminated at the end instead") }
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
        tolerant("long-press the Settings icon") { icon.press(forDuration: 1.2) }
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
        reportAddress(capture(safari, Self.safariId, "Safari portrait, page loaded"), "portrait")
        tolerant("rotate to landscape left") { XCUIDevice.shared.orientation = .landscapeLeft }
        await pause(2.5)
        note("device orientation now \(XCUIDevice.shared.orientation.rawValue) (landscapeLeft is 3)")
        reportAddress(capture(safari, Self.safariId, "Safari landscape left"), "landscape")
        tolerant("rotate back to portrait") { XCUIDevice.shared.orientation = .portrait }
        await pause(2)
        reportAddress(capture(safari, Self.safariId, "Safari portrait again"), "portrait again")
        note("not covered: a neighbouring tab's address capsule (only one tab is open)")
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

        lines.append("readings (seconds since the probe started, the text read, the change since the reading before, what happened in between):")
        var byKind: [String: [Int]] = [:]
        for (i, r) in readings.enumerated() {
            var delta: Int?
            if i > 0, let now = r.count, let before = readings[i - 1].count { delta = now - before }
            if let delta { byKind[r.kind, default: []].append(delta) }
            lines.append(String(format: "    %7.2f s  ", r.at) + "\"\(r.label)\"  change \(delta.map { String($0) } ?? "-")  after: \(r.after)")
        }
        for kind in ["read", "idle", "snapshot", "screenshot", "capture"] {
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
        note("the page logs focus, blur, focusin, focusout, visibilitychange, scroll, selectionchange, resize, pointerdown and touchstart events and runs a 50 ms heartbeat (beats, the largest gap, gaps over 100 and 250 ms, the last big gaps as page-time:gap ms); its line is re-rendered every second and read here with one plain XCUITest query")
        await pause(3)
        var readings: [Reading] = []
        readCounter(out, "start", "page loaded, 3 s quiet", &readings)
        readCounter(out, "read", "one plain read", &readings)
        await pause(2)
        readCounter(out, "idle", "2 s quiet, one plain read", &readings)
        let runner = XCUITestPhoneRunner()
        runner.noteActivated(Self.safariId)
        runner.onObservation = { [weak self] record in self?.attachObservation(record) }
        for i in 1...5 {
            do {
                _ = try await runner.snapshot(includeScreenshot: false)
            } catch {
                note("runner snapshot \(i) threw: \(error)")
            }
            await pause(1)
        }
        runner.onObservation = nil
        readCounter(out, "snapshot", "5 runner snapshots 1 s apart, one plain read", &readings)
        await pause(3)
        readCounter(out, "idle", "3 s quiet, one plain read", &readings)
        lines.append("page readings (seconds since the probe started, then the page's own line):")
        for r in readings {
            lines.append(String(format: "    %7.2f s  ", r.at) + "after \(r.after): \"\(r.label)\"")
        }
        note("how to read them: compare the event counters and the heartbeat's gaps between the readings before and after the snapshots; page time (t=) is performance.now() in ms. No change beyond what the plain reads themselves cause means no snapshot was observable from the page on this OS")
        let log = server.log
        note("server log: " + (log.isEmpty ? "(no request reached the server)" : log.joined(separator: " | ")))
    }

    /// The probe page for (i): event counters and a heartbeat, shown as one line of text.
    private static let probePage = """
    <!doctype html>
    <html><head><meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Jarvis QA probe</title>
    <style>
    body { font: 18px -apple-system, sans-serif; margin: 20px; }
    p { word-break: break-all; }
    </style></head>
    <body>
    <h1>Snapshot probe</h1>
    <p id="out">probe t=0 starting</p>
    <script>
    var counts = { focus: 0, blur: 0, focusin: 0, focusout: 0, visibilitychange: 0, scroll: 0, selectionchange: 0, resize: 0, pointerdown: 0, touchstart: 0 };
    function bump(name) { return function () { counts[name] += 1; }; }
    ['focus', 'blur', 'scroll', 'resize'].forEach(function (n) { window.addEventListener(n, bump(n), true); });
    ['focusin', 'focusout', 'visibilitychange', 'selectionchange', 'pointerdown', 'touchstart'].forEach(function (n) { document.addEventListener(n, bump(n), true); });
    var beats = 0, maxGap = 0, gaps100 = 0, gaps250 = 0, last = performance.now(), big = [];
    setInterval(function () {
      var now = performance.now(), gap = now - last;
      last = now;
      beats += 1;
      if (gap > maxGap) { maxGap = gap; }
      if (gap > 100) { gaps100 += 1; big.push(Math.round(now) + ':' + Math.round(gap)); if (big.length > 6) { big.shift(); } }
      if (gap > 250) { gaps250 += 1; }
    }, 50);
    function render() {
      var parts = ['probe t=' + Math.round(performance.now()), 'beats=' + beats, 'maxgap=' + Math.round(maxGap), 'gaps100=' + gaps100, 'gaps250=' + gaps250];
      Object.keys(counts).forEach(function (k) { parts.push(k + '=' + counts[k]); });
      parts.push('big=' + (big.length ? big.join(',') : 'none'));
      document.getElementById('out').textContent = parts.join(' ');
    }
    setInterval(render, 1000);
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
        if let menu = hit.ancestors.last(where: menuish) {
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

    /// Label, value or identifier contains `text` (case-insensitive).
    private static func mentions(_ text: String) -> (RawElement) -> Bool {
        { e in
            e.label.localizedCaseInsensitiveContains(text) || (e.value ?? "").localizedCaseInsensitiveContains(text)
                || e.identifier.localizedCaseInsensitiveContains(text)
        }
    }
}
