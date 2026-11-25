import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Probe round 3 on system apps and web content (the audit's P5, P6, P7, P12): Safari and an
/// in-app `WKWebView` on local pages (`Probe3Server`, loopback only), Messages' compose screen
/// (never sent), a Notes row under the runner's own swipes, Contacts' delete confirmation
/// (never confirmed), the app-icon badge, Notification Centre, Control Centre, the App
/// Switcher and swipe-back in Settings. Conventions: `Probe3Case`.
///
/// Simulator time (typical): P5a ~50 s, P5b ~70 s, P5c ~50 s, P6a ~90 s, P6b ~90 s, P6c ~50 s,
/// P7 ~50 s, P12a ~80 s, P12b ~35 s.
final class TreeFactProbe3SystemTests: Probe3Case {
    private static let messagesId = "com.apple.MobileSMS"
    private static let notesId = "com.apple.mobilenotes"
    private static let contactsId = "com.apple.MobileAddressBook"

    // MARK: - Web pages (served from the test process on 127.0.0.1)

    /// The probe page: a tap target and a mirrored field (P7), links, a cross-origin iframe
    /// (`framePort`: another port is another origin), JavaScript dialogs, a modal `<dialog>`,
    /// `<select>`, date and file inputs, and a form posting to `/submit`.
    static func webPage(framePort: UInt16) -> String {
        """
        <!doctype html>
        <html><head><meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Probe3 web</title>
        <style>
        body { font: 16px -apple-system, sans-serif; margin: 10px; }
        button, select, input { font-size: 16px; margin: 3px 0; }
        p { margin: 5px 0; }
        iframe { width: 300px; height: 110px; border: 1px solid #888; }
        </style></head>
        <body>
        <h1 style="font-size: 22px; margin: 4px 0;">Web probe</h1>
        <p><button id="tap" onclick="taps += 1; document.getElementById('taps').textContent = 'taps:' + taps">Probe tap target</button> <span id="taps">taps:0</span></p>
        <p><input id="mirror" aria-label="Probe mirror" placeholder="Probe mirror" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false" oninput="document.getElementById('typed').textContent = 'typed:' + this.value"> <span id="typed">typed:</span></p>
        <p><a href="/linked-page">Probe plain link</a> and <a href="https://example.com/probe-external">Probe external link</a></p>
        <iframe src="http://127.0.0.1:\(framePort)/frame" title="Probe frame"></iframe>
        <p><button onclick="alert('Probe alert text')">Probe alert</button> <button onclick="out('confirm:' + confirm('Probe confirm text'))">Probe confirm</button> <button onclick="out('prompt:' + prompt('Probe prompt text', 'x'))">Probe prompt</button> <button onclick="document.getElementById('dlg').showModal()">Probe dialog</button></p>
        <p id="result">result:none</p>
        <dialog id="dlg"><p>Probe dialog text</p><button onclick="document.getElementById('dlg').close()">Close probe dialog</button></dialog>
        <p><select aria-label="Probe select"><option>Alpha</option><option selected>Beta</option><option>Gamma</option></select> <input type="date" aria-label="Probe date" value="2026-11-25"></p>
        <p><input type="file" aria-label="Probe file"></p>
        <form method="post" action="/submit"><input name="name" aria-label="Probe name" placeholder="Probe name" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck="false"> <button type="submit">Probe submit</button></form>
        <script>
        var taps = 0;
        function out(t) { document.getElementById('result').textContent = 'result:' + t; }
        </script>
        </body></html>
        """
    }

    static let framePage = """
    <!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Probe frame</title></head>
    <body style="font: 15px -apple-system, sans-serif; margin: 6px;">
    <p>Probe frame text</p>
    <button onclick="document.getElementById('fs').textContent = 'frame tapped'">Probe frame button</button>
    <input aria-label="Probe frame field" placeholder="Probe frame field">
    <p id="fs">frame idle</p>
    </body></html>
    """

    /// Two servers: the iframe's (its own port, so its own origin) and the main page's.
    @MainActor
    private func startServers() -> (main: Probe3Server, frame: Probe3Server)? {
        do {
            let frame = try Probe3Server(page: Self.framePage)
            let main = try Probe3Server(page: Self.webPage(framePort: frame.port))
            note("main page \(main.baseURL)/, its iframe from \(frame.baseURL)/frame (another port: another origin); loopback only")
            return (main, frame)
        } catch {
            note("the local test servers did not start: \(error)")
            return nil
        }
    }

    // MARK: - P6a: Safari: links, cross-origin iframe, JavaScript dialogs, <dialog>

    @MainActor
    func testP06a_SafariLinksFrameDialogs() async throws {
        executionTimeAllowance = 240
        begin("p6a-safari-page", "In Safari: what role do links have and does a link carry its URL; is a cross-origin iframe's content in the tree, nested how, with screen frames; whose tree holds alert(), confirm(), prompt() and a modal <dialog>, with what roles?")
        let reason = await probeSafariPage()
        await finish(terminating: [Self.safariId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeSafariPage() async -> String? {
        guard let servers = startServers() else { return "the local test servers did not start" }
        defer {
            servers.main.stop()
            servers.frame.stop()
        }
        guard let safari = await openInSafari(servers.main.baseURL + "/", waitFor: "Web probe") else { return "Safari did not open the probe page" }
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        _ = waitFor(Self.node(in: safari, labelled: "Probe frame button"), "the iframe's button", 10)
        await pause(1)
        let page = capture(safari, Self.safariId, "page", "The probe page in Safari: links (role, URL anywhere?), the cross-origin iframe's content (in the tree? nested how? screen frames?)")
        webPageFacts(safari, page, "Safari")
        attachQuery(safari, "page", "The probe page in Safari: what does XCUITest's query view reach (the cross-origin iframe's content)?")
        await runnerLook("Safari showing the probe page", "page")
        let dialogs = [("Probe alert", "js-alert", "Probe alert text"), ("Probe confirm", "js-confirm", "Probe confirm text"), ("Probe prompt", "js-prompt", "Probe prompt text")]
        for (button, slug, text) in dialogs {
            await jsDialog(safari, board, button: button, slug: slug, text: text, appId: Self.safariId)
        }
        await htmlDialog(safari, appId: Self.safariId)
        note("server log: " + servers.main.log.joined(separator: " | ") + " || frame server: " + servers.frame.log.joined(separator: " | "))
        return nil
    }

    /// Links, the cross-origin iframe and the page's WebView nodes in `tree`.
    @MainActor
    private func webPageFacts(_ app: XCUIApplication, _ tree: RawElement?, _ place: String) {
        guard let tree else {
            fact("\(place): no page tree (the capture failed)")
            return
        }
        let links = report(tree, "\(place): Link nodes", limit: 12) { $0.elementType == XCUIElementTypeCode.link }
        let plain = Self.hits(tree, Self.mentions("Probe plain link"))
        let external = Self.hits(tree, Self.mentions("Probe external link"))
        let urls = report(tree, "\(place): nodes whose label, value or identifier holds a link's URL (linked-page or example.com)") { e in
            Self.mentions("linked-page")(e) || Self.mentions("example.com")(e)
        }
        let plainText = Self.briefs(Array(plain.prefix(1)), empty: "not found")
        let externalText = Self.briefs(Array(external.prefix(1)), empty: "not found")
        fact("\(place): Link nodes \(links.count); the plain link: \(plainText); the external link: \(externalText); a link's URL in any label, value or identifier: \(Self.yesNo(!urls.isEmpty))")
        let webViews = report(tree, "\(place): WebView nodes") { $0.elementType == XCUIElementTypeCode.webView }
        let frameNodes = report(tree, "\(place): nodes of the cross-origin iframe (Probe frame …)", limit: 12, Self.mentions("Probe frame"))
        if let button = frameNodes.first(where: { $0.element.label == "Probe frame button" }) {
            let depth = button.ancestors.filter { $0.elementType == XCUIElementTypeCode.webView }.count
            let chain = Self.ancestry(button, 6)
            let query = queryFrame(Self.node(in: app, labelled: "Probe frame button")).map { Self.frameText($0) } ?? "not found by query"
            fact("\(place): the iframe's button is in the tree: \(Self.brief(button.element)), snapshot frame \(Self.rectText(button.element.frame)) (on screen \(Self.yesNo(Self.onScreen(button.element.frame, tree.frame)))), query frame \(query); WebView ancestors \(depth) of \(webViews.count) WebView nodes; nearest ancestors: \(chain)")
        } else {
            let query = queryFrame(Self.node(in: app, labelled: "Probe frame button")).map { Self.frameText($0) } ?? "not found either"
            fact("\(place): the iframe's button is not in the snapshot (\(frameNodes.count) node(s) mention Probe frame); by query: \(query)")
        }
        let field = frameNodes.first { $0.element.label == "Probe frame field" || $0.element.placeholder == "Probe frame field" }
        let fieldText = field.map { "\(Self.brief($0.element)) \(Self.rectText($0.element.frame))" } ?? "not in the snapshot"
        fact("\(place): the iframe's text field: \(fieldText)")
    }

    /// A JavaScript dialog: whose tree holds it, with what roles; dismissed with Cancel, Close
    /// or OK.
    @MainActor
    private func jsDialog(_ app: XCUIApplication, _ board: XCUIApplication, button: String, slug: String, text: String, appId: String) async {
        let trigger = app.buttons[button].firstMatch
        guard waitFor(trigger, "the \(button) button", 5) else {
            fact("\(button): could not produce (no button on the page)")
            return
        }
        tolerant("tap \(button)") { trigger.tap() }
        let owner = await waitForAlert([(appId, app), (Self.springboardId, board)], 5)
        await pause(0.5)
        let tree = capture(app, appId, slug, "After \(button): does \(appId)'s tree hold the JavaScript dialog, with what roles?")
        let boardTree = capture(board, Self.springboardId, "\(slug)-springboard", "After \(button): does SpringBoard's tree hold it?")
        let inApp = report(tree, "\(button): nodes mentioning its text (\(appId))", Self.mentions(text))
        let inBoard = report(boardTree, "\(button): nodes mentioning its text (SpringBoard)", Self.mentions(text))
        let modal = report(tree, "\(button): modal-like nodes (\(appId))", Self.modalish)
        if slug == "js-prompt" {
            report(tree, "\(button): TextField nodes") { $0.elementType == XCUIElementTypeCode.textField }
        }
        let look = await runnerLook("the \(button) dialog up", slug)
        var labels: [String] = []
        if let (ownerId, ownerApp) = owner { labels = readAlert(ownerApp, ownerId, button) }
        let holder = inApp.first.map { hit in "\(Self.brief(hit.element)), ancestors \(Self.ancestry(hit, 4))" } ?? "none"
        let ownerText = owner?.0 ?? "neither \(appId) nor SpringBoard"
        let modalText = Self.typeNames(modal)
        let lookApp = look?.appBundleId ?? "?"
        fact("\(button): an Alert by query in \(ownerText); its text in \(appId)'s snapshot: \(inApp.count) node(s) (first: \(holder)); in SpringBoard's: \(inBoard.count); modal-like nodes in \(appId)'s: \(modalText); buttons \(labels); the runner's look is of \(lookApp)")
        let choice = ["Cancel", "Close", "OK"].first { labels.contains($0) }
        if let (_, ownerApp) = owner, let choice {
            await tapInAlert(ownerApp, choice, "dismiss the \(button) dialog")
        } else {
            tapFirstButton(app, ["Cancel", "Close", "OK"], "dismiss the \(button) dialog")
        }
        await pause(0.8)
        note("\(button): the page's result line is now \"\(labelNow(Self.node(in: app, containing: "result:")))\"")
    }

    /// A modal `<dialog>` (showModal): its role, and whether the page behind stays in the tree.
    @MainActor
    private func htmlDialog(_ app: XCUIApplication, appId: String) async {
        let open = app.buttons["Probe dialog"].firstMatch
        guard waitFor(open, "the Probe dialog button", 5) else {
            fact("<dialog>: could not produce (no button)")
            return
        }
        tolerant("open the <dialog>") { open.tap() }
        await pause(1.2)
        let tree = capture(app, appId, "html-dialog", "A modal <dialog> (showModal) open: what role does it get, and is the page behind still in the tree and marked covered?")
        let text = report(tree, "<dialog>: nodes mentioning Probe dialog text", Self.mentions("Probe dialog text"))
        let modal = report(tree, "<dialog>: modal-like nodes", Self.modalish)
        let behind = tree.map { !Self.hits($0, Self.mentions("Probe tap target")).isEmpty } ?? false
        let behindHittable = hittableNow(app.buttons["Probe tap target"].firstMatch)
        let textLine = text.first.map { hit in "\(Self.brief(hit.element)) under \(Self.ancestry(hit, 3))" } ?? "not in the tree"
        let modalText = Self.typeNames(modal)
        fact("<dialog> open: its text \(textLine); modal-like nodes \(modal.count) (\(modalText)); the page behind in the tree \(Self.yesNo(behind)), its button hittable by query \(Self.yesNo(behindHittable))")
        await runnerLook("a modal <dialog> open", "html-dialog")
        tapFirstButton(app, ["Close probe dialog"], "close the <dialog>")
        await pause(0.8)
    }

    // MARK: - P6b: Safari: select, date and file inputs, Return-submit, Basic auth

    @MainActor
    func testP06b_SafariInputsSubmitAuth() async throws {
        executionTimeAllowance = 240
        begin("p6b-safari-inputs", "In Safari: what do <select>, <input type=date> and <input type=file> open, and in whose tree; does the runner's own type + Return submit a web form (did a POST arrive); whose tree holds an HTTP Basic auth challenge, with what roles?")
        let reason = await probeSafariInputs()
        await finish(terminating: [Self.safariId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeSafariInputs() async -> String? {
        guard let servers = startServers() else { return "the local test servers did not start" }
        defer {
            servers.main.stop()
            servers.frame.stop()
        }
        guard let safari = await openInSafari(servers.main.baseURL + "/", waitFor: "Web probe") else { return "Safari did not open the probe page" }
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let inputs = [("Probe select", "select", ["Alpha", "Gamma"]), ("Probe date", "date-input", ["Reset", "Done"]),
                      ("Probe file", "file-input", ["Photo Library", "Take Photo", "Choose File", "Choose Files"])]
        for (label, slug, markers) in inputs {
            await webInput(safari, board, label: label, slug: slug, markers: markers)
        }
        await submitViaRunner(safari, Self.safariId, servers.main, "Safari")
        await basicAuth(safari, board, servers.main)
        note("server log: " + servers.main.log.joined(separator: " | "))
        return nil
    }

    @MainActor
    private func webInput(_ app: XCUIApplication, _ board: XCUIApplication, label: String, slug: String, markers: [String]) async {
        var control = Self.node(in: app, labelled: label)
        if !existsNow(control) { control = Self.node(in: app, containing: label) }
        guard existsNow(control) else {
            fact("\(label): could not produce (no node labelled \(label))")
            return
        }
        let before = queryFrame(control).map { Self.frameText($0) } ?? "-"
        tolerant("tap \(label)") { control.tap() }
        await pause(1.5)
        let tree = capture(app, Self.safariId, slug, "After tapping \(label): what opens (menu, popover, picker, sheet), in whose tree, with what roles?")
        let boardTree = capture(board, Self.springboardId, "\(slug)-springboard", "After tapping \(label): does SpringBoard's tree hold what opened?")
        let modal = report(tree, "\(label): modal-like nodes", Self.modalish)
        let items = report(tree, "\(label): MenuItem, PickerWheel, DatePicker and CollectionView nodes", limit: 12,
                           Self.isType([XCUIElementTypeCode.menuItem, XCUIElementTypeCode.pickerWheel, XCUIElementTypeCode.datePicker, XCUIElementTypeCode.collectionView]))
        let marked = report(tree, "\(label): nodes labelled like what opens (\(markers.joined(separator: ", ")))", limit: 12) { markers.contains($0.label) }
        let inBoard = boardTree.map { tree in Self.hits(tree) { markers.contains($0.label) }.count } ?? 0
        let look = await runnerLook("after tapping \(label)", slug)
        let controlNode = tree.flatMap { Self.hits($0, Self.mentions(label)).first?.element }
        let controlText = controlNode.map { Self.brief($0) } ?? "not in the tree after the tap"
        let modalText = Self.typeNames(modal)
        let itemText = Self.briefs(items)
        let markedText = Self.briefs(marked)
        let lookApp = look?.appBundleId ?? "?"
        fact("\(label) (control \(controlText), query frame before the tap \(before)): opens modal-like nodes \(modalText); menu/picker nodes: \(itemText); marker nodes in Safari's tree \(marked.count) (\(markedText)), in SpringBoard's \(inBoard); the runner's look is of \(lookApp)")
        let heading = Self.node(in: app, labelled: "Web probe")
        tolerant("tap the page heading to dismiss") { if heading.exists { heading.tap() } }
        await pause(1)
        let still = markers.contains { existsNow(Self.node(in: app, labelled: $0)) }
        if still {
            tapFirstButton(app, ["Done", "Cancel", "Close"], "dismiss what \(label) opened")
            await pause(1)
        }
    }

    /// The runner's own path for typing into a web form and pressing Return: a tap on the
    /// field's centre from its look, `typeText`, `pressKey(.return)`; then whether a POST came.
    @MainActor
    private func submitViaRunner(_ app: XCUIApplication, _ appId: String, _ server: Probe3Server, _ place: String) async {
        let runner = XCUITestPhoneRunner()
        let look = await runnerLook("\(place) before typing into the form", "before-submit", runner: runner)
        guard let root = look?.root else {
            fact("\(place) Return-submit: could not produce (no runner look)")
            return
        }
        let fields = Self.findAll(root) { node in
            node.role == .textField && ((node.label ?? "").contains("Probe name") || (node.placeholder ?? "").contains("Probe name") || (node.value ?? "").contains("Probe name"))
        }
        guard let field = fields.first else {
            fact("\(place) Return-submit: the form's text field is not in the runner's look")
            return
        }
        let point = Self.centre(field.frame)
        await runnerDo(runner, .tap(x: point.x, y: point.y), expecting: appId, "tap the form's field at the centre of its frame \(Self.rectText(field.frame))")
        await pause(1.2)
        let keyboard = capture(app, appId, "form-keyboard", "\(place): the keyboard for a web form field: the Return key's label and identifier")
        let key = reportKeyboard(keyboard, "\(place) form field")
        let keyText = key.map { Self.brief($0) } ?? "not located"
        fact("\(place) web form field: the Return key is \(keyText)")
        let before = server.submissions.count
        await runnerDo(runner, .typeText(text: "Return probe"), expecting: appId, "type into the form's field")
        await runnerDo(runner, .pressKey(key: .`return`), expecting: appId, "press Return (typeText \"\\n\")")
        var arrived = false
        let deadline = Date().addingTimeInterval(6)
        while Date() < deadline {
            if server.submissions.count > before {
                arrived = true
                break
            }
            await pause(0.3)
        }
        var last = "-"
        if let form = server.submissions.last {
            last = form.keys.sorted().map { key in "\(key)=\(form[key] ?? "")" }.joined(separator: " ")
        }
        fact("\(place): the runner's type + Return in a web form field submitted the form (a POST arrived within 6 s): \(Self.yesNo(arrived)); last submission: \(last)")
        await pause(1)
        capture(app, appId, "after-return", "\(place) after the runner's Return: did the page change to the form's answer (\"Probe form received\")?")
    }

    /// An HTTP Basic auth challenge: whose tree holds the sign-in prompt, with what roles.
    @MainActor
    private func basicAuth(_ safari: XCUIApplication, _ board: XCUIApplication, _ server: Probe3Server) async {
        guard let url = URL(string: server.baseURL + "/auth") else { return }
        tolerant("open the auth page") { XCUIDevice.shared.system.open(url) }
        await pause(2)
        let owner = await waitForAlert([(Self.safariId, safari), (Self.springboardId, board)], 6)
        await pause(0.5)
        let tree = capture(safari, Self.safariId, "basic-auth", "An HTTP Basic auth challenge in Safari: is the sign-in prompt in Safari's tree, as what role, with what fields and buttons?")
        let boardTree = capture(board, Self.springboardId, "basic-auth-springboard", "An HTTP Basic auth challenge: does SpringBoard's tree hold the prompt?")
        let modal = report(tree, "Basic auth: modal-like nodes (Safari)", Self.modalish)
        let fields = report(tree, "Basic auth: TextField and SecureTextField nodes (Safari)", Self.isType([XCUIElementTypeCode.textField, XCUIElementTypeCode.secureTextField]))
        let named = report(tree, "Basic auth: nodes naming the realm, the host or Log In", limit: 12) { e in
            Self.mentions("Probe realm")(e) || Self.mentions("127.0.0.1")(e) || Self.mentions("Log In")(e) || Self.mentions("Sign In")(e)
        }
        let inBoard = boardTree.map { Self.hits($0, Self.modalish).count } ?? 0
        let look = await runnerLook("the Basic auth prompt up", "basic-auth")
        var labels: [String] = []
        if let (ownerId, ownerApp) = owner { labels = readAlert(ownerApp, ownerId, "Basic auth") }
        let ownerText = owner?.0 ?? "neither Safari nor SpringBoard"
        let modalText = Self.typeNames(modal)
        let fieldText = Self.briefs(fields)
        let lookApp = look?.appBundleId ?? "?"
        fact("HTTP Basic auth: an Alert by query in \(ownerText); Safari's modal-like nodes: \(modalText); fields: \(fieldText); nodes naming realm/host/Log In: \(named.count); SpringBoard modal-like nodes \(inBoard); buttons by query \(labels); the runner's look is of \(lookApp)")
        if let (_, ownerApp) = owner, labels.contains("Cancel") {
            await tapInAlert(ownerApp, "Cancel", "cancel the sign-in")
        } else {
            tapFirstButton(safari, ["Cancel", "Close"], "cancel the sign-in")
        }
        await pause(1)
        note("auth requests: " + server.log.filter { $0.contains("/auth") }.joined(separator: " | "))
    }

    // MARK: - P6c: a WKWebView in the app

    @MainActor
    func testP06c_WebViewInApp() async throws {
        executionTimeAllowance = 180
        begin("p6c-webview", "In a WKWebView inside the Playground: is there a WebView node, is the host app's own chrome distinguishable from page content, is the cross-origin iframe's content there, and does the runner's type + Return submit the page's form?")
        let reason = await probeWebView()
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeWebView() async -> String? {
        guard let servers = startServers() else { return "the local test servers did not start" }
        defer {
            servers.main.stop()
            servers.frame.stop()
        }
        let app = launchPlayground(extraArguments: ["-jarvis-probe-url", servers.main.baseURL + "/"])
        guard await openProbe3(app, "probe3.web") else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Web view screen not reached?")
            return "the Web view screen was not reached"
        }
        _ = waitFor(Self.node(in: app, labelled: "Web probe"), "the page heading in the web view", 15)
        _ = waitFor(Self.node(in: app, labelled: "Probe frame button"), "the iframe's button in the web view", 8)
        await pause(1)
        let tree = capture(app, Self.appBundleId, "webview", "The probe page in a WKWebView under the host's own chrome: is there a WebView node; can host chrome and page content be told apart; is the cross-origin iframe there?")
        webPageFacts(app, tree, "WKWebView")
        if let tree {
            let chrome = Self.hits(tree) { $0.identifier == "probe3.web.chrome" || $0.identifier == "probe3.web.hostbutton" }
            let heading = Self.hits(tree, Self.mentions("Web probe"))
            let chromeParts: [String] = chrome.map { hit in
                let under = Self.yesNo(hit.ancestors.contains { $0.elementType == XCUIElementTypeCode.webView })
                return "\(Self.brief(hit.element)) (WebView ancestor \(under))"
            }
            let headingParts: [String] = heading.map { hit in
                let under = Self.yesNo(hit.ancestors.contains { $0.elementType == XCUIElementTypeCode.webView })
                return "\(Self.brief(hit.element)) (WebView ancestor \(under))"
            }
            let chromeText = chromeParts.joined(separator: "; ")
            let headingText = headingParts.joined(separator: "; ")
            fact("WKWebView: host chrome nodes: \(chromeText); page heading: \(headingText)")
        }
        attachQuery(app, "webview", "The WKWebView screen: what does XCUITest's query view reach?")
        await runnerLook("the WKWebView screen", "webview")
        await submitViaRunner(app, Self.appBundleId, servers.main, "WKWebView")
        note("server log: " + servers.main.log.joined(separator: " | "))
        return nil
    }

    // MARK: - P7: landscape taps and typing

    @MainActor
    func testP07_LandscapeTapAndType() async throws {
        executionTimeAllowance = 180
        begin("p7-landscape", "In landscape (Safari; the Playground is portrait-only), does the runner's tap at the centre of a control's reported frame reach that control, and does typing into a field tapped that way arrive?")
        let reason = await probeLandscape()
        await finish(terminating: [Self.safariId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeLandscape() async -> String? {
        defer { if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait } }
        guard let servers = startServers() else { return "the local test servers did not start" }
        defer {
            servers.main.stop()
            servers.frame.stop()
        }
        guard let safari = await openInSafari(servers.main.baseURL + "/", waitFor: "Web probe") else { return "Safari did not open the probe page" }
        tolerant("rotate to landscape left") { XCUIDevice.shared.orientation = .landscapeLeft }
        await pause(2.5)
        note("device orientation now \(XCUIDevice.shared.orientation.rawValue) (landscapeLeft is 3)")
        let runner = XCUITestPhoneRunner()
        let look = await runnerLook("Safari in landscape", "landscape", runner: runner)
        capture(safari, Self.safariId, "landscape", "Safari in landscape: the frames XCUITest's snapshot gives the page's controls (compare with the runner's look and the query frames)")
        guard let root = look?.root else {
            fact("landscape: could not produce (no runner look)")
            return "no runner look in landscape"
        }
        let target = Self.findAll(root) { $0.role == .button && ($0.label ?? "").contains("Probe tap target") }.first
        let tapsText = Self.node(in: safari, containing: "taps:")
        if let target {
            let query = queryFrame(safari.buttons["Probe tap target"].firstMatch).map { Self.frameText($0) } ?? "not found by query"
            let point = Self.centre(target.frame)
            await runnerDo(runner, .tap(x: point.x, y: point.y), expecting: Self.safariId, "tap the centre of the tap target's frame from the runner's look \(Self.rectText(target.frame))")
            await pause(1.2)
            let after = labelNow(tapsText)
            fact("landscape tap: the runner's frame \(Self.rectText(target.frame)), query frame \(query); after the runner's tap at its centre the page shows \"\(after)\" (reached: \(Self.yesNo(after == "taps:1")))")
        } else {
            fact("landscape tap: the tap target is not in the runner's look")
        }
        let look2 = await runnerLook("Safari in landscape, before typing", "landscape-before-typing", runner: runner)
        let mirror = look2?.root.flatMap { root in
            Self.findAll(root) { $0.role == .textField && (($0.label ?? "").contains("Probe mirror") || ($0.placeholder ?? "").contains("Probe mirror")) }.first
        }
        if let mirror {
            let point = Self.centre(mirror.frame)
            await runnerDo(runner, .tap(x: point.x, y: point.y), expecting: Self.safariId, "tap the centre of the mirror field's frame \(Self.rectText(mirror.frame))")
            await pause(1.2)
            await runnerDo(runner, .typeText(text: "land"), expecting: Self.safariId, "type into the mirror field")
            await pause(1)
            let typed = labelNow(Self.node(in: safari, containing: "typed:"))
            fact("landscape typing: after the runner's tap on the field's centre and typeText(\"land\") the page shows \"\(typed)\" (arrived: \(Self.yesNo(typed == "typed:land")))")
        } else {
            fact("landscape typing: the mirror field is not in the runner's look")
        }
        capture(safari, Self.safariId, "landscape-after", "Safari in landscape after the runner's tap and typing")
        tolerant("rotate back to portrait") { XCUIDevice.shared.orientation = .portrait }
        await pause(1.5)
        return nil
    }

    // MARK: - P5a: Messages' compose screen (never sent)

    @MainActor
    func testP05a_MessagesCompose() async throws {
        executionTimeAllowance = 180
        begin("p5a-messages", "In Messages' new-message screen with a recipient and a body typed: what are the send button's role, label and identifier, and what is the Return key there? (Nothing is sent.)")
        let reason = await probeMessages()
        await finish(terminating: [Self.messagesId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeMessages() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [Self.messagesId])
        let messages = XCUIApplication(bundleIdentifier: Self.messagesId)
        tolerant("open Messages") { messages.activate() }
        guard messages.wait(for: .runningForeground, timeout: 15) else { return "Messages did not reach the foreground" }
        await pause(2)
        for _ in 0..<2 {
            if tapFirstButton(messages, ["Continue", "Not Now"], "close a Messages first-launch screen") == nil { break }
            await pause(1.5)
        }
        capture(messages, Self.messagesId, "list", "Messages' first screen: where is the compose button (role, label, identifier)?")
        let compose = messages.buttons.matching(NSPredicate(format: "label == %@ OR label == %@ OR label == %@ OR identifier CONTAINS[c] %@", "Compose", "New Message", "compose", "compose")).firstMatch
        guard waitFor(compose, "the compose button", 5) else { return "no compose button found in Messages" }
        tolerant("tap compose") { compose.tap() }
        await pause(2)
        tolerant("type the recipient") { messages.typeText("5555550123") }
        await pause(1.5)
        let predicate = NSPredicate(format: "label CONTAINS[c] %@ OR placeholderValue CONTAINS[c] %@ OR identifier CONTAINS[c] %@", "message", "message", "message")
        let bodyView = messages.textViews.matching(predicate).firstMatch
        let body = existsNow(bodyView) ? bodyView : messages.textFields.matching(predicate).firstMatch
        tolerant("focus the message body") { body.tap() }
        await pause(1)
        tolerant("type the body") { messages.typeText("Probe body 8812") }
        await pause(1.5)
        let tree = capture(messages, Self.messagesId, "compose-filled", "Messages' new message with a recipient and a body: the send button (role, label, identifier), the Return key, the recipient and body nodes")
        let send = report(tree, "send controls (label or identifier contains send)") { e in
            e.label.localizedCaseInsensitiveContains("send") || e.identifier.localizedCaseInsensitiveContains("send")
        }
        report(tree, "recipient nodes (mentioning 555)", Self.mentions("555"))
        report(tree, "body nodes (mentioning Probe body)", Self.mentions("Probe body"))
        let key = reportKeyboard(tree, "Messages body")
        if let tree { inputHostFacts(tree, "Messages' keyboard") }
        let look = await runnerLook("Messages' compose screen filled in", "compose-filled")
        let typedSend = look?.root.map { root in Self.findAll(root) { ($0.label ?? "").localizedCaseInsensitiveContains("send") || ($0.identifier ?? "").localizedCaseInsensitiveContains("send") }.map { Self.describeNode($0) } } ?? []
        let sendText = Self.briefs(send)
        let typedText = typedSend.isEmpty ? "none" : typedSend.joined(separator: "; ")
        fact("Messages compose: send controls in the raw tree: \(sendText); in the runner's typed tree: \(typedText)")
        let keyText = key.map { Self.brief($0) } ?? "not located"
        fact("Messages compose: the Return key (bottom-right key of the Keyboard node) is \(keyText)")
        tolerant("delete the body text") { messages.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 16)) }
        await pause(0.5)
        tapFirstButton(messages, ["Cancel", "Close"], "cancel the new message")
        await pause(1.5)
        if existsNow(messages.buttons["Delete Draft"].firstMatch) {
            tapFirstButton(messages, ["Delete Draft"], "delete the draft")
        }
        return nil
    }

    // MARK: - P5b: a Notes row under the runner's swipes

    @MainActor
    func testP05b_NotesRowSwipe() async throws {
        executionTimeAllowance = 180
        begin("p5b-notes", "A note's row in Notes' list: what is the row in the tree; does the runner's own swipe (the core's path for a swipe left on the row, then a full-width swipe) reveal swipe actions (roles, labels) or delete the note?")
        let reason = await probeNotes()
        await finish(terminating: [Self.notesId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeNotes() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [Self.notesId])
        let notes = XCUIApplication(bundleIdentifier: Self.notesId)
        tolerant("open Notes") { notes.activate() }
        guard notes.wait(for: .runningForeground, timeout: 15) else { return "Notes did not reach the foreground" }
        await pause(2)
        for _ in 0..<3 {
            if tapFirstButton(notes, ["Continue", "Not Now"], "close a Notes first-launch screen") == nil { break }
            await pause(1.5)
        }
        capture(notes, Self.notesId, "start", "Notes' first screen: where is the New Note button?")
        let compose = notes.buttons.matching(NSPredicate(format: "label CONTAINS[c] %@ OR label CONTAINS[c] %@ OR identifier CONTAINS[c] %@", "New Note", "Compose", "compose")).firstMatch
        guard waitFor(compose, "the New Note button", 5) else { return "no New Note button found in Notes" }
        tolerant("tap New Note") { compose.tap() }
        await pause(2)
        tolerant("type the note") { notes.typeText("Probe note 3317\nProbe note body") }
        await pause(1)
        capture(notes, Self.notesId, "editor", "The new note in Notes' editor")
        if tapFirstButton(notes, ["Notes", "Back"], "go back to the list") == nil {
            tolerant("tap the navigation bar's first button") {
                let first = notes.navigationBars.buttons.element(boundBy: 0)
                if first.exists { first.tap() }
            }
        }
        await pause(1.5)
        let list = capture(notes, Self.notesId, "list", "Notes' list with the new note: what is its row (role, label, identifier, frame)?")
        let row = list.flatMap { tree in
            Self.hits(tree) { $0.elementType == XCUIElementTypeCode.cell && !Self.hits($0, Self.mentions("Probe note 3317")).isEmpty }.last
        }
        guard let row else {
            fact("Notes: the new note's row was not found in the list")
            return "the new note's row was not found in Notes' list"
        }
        fact("Notes: the note's row: \(Self.brief(row.element)) \(Self.rectText(row.element.frame))")
        let runner = XCUITestPhoneRunner()
        let look = await runnerLook("Notes' list with the new note", "list", runner: runner)
        guard let screen = look?.screenSize else {
            fact("Notes swipes: could not produce (no runner look)")
            return nil
        }
        let frame = look?.root.flatMap { Self.rowFrame($0, "Probe note 3317", screen) } ?? row.element.frame
        if let path = Self.coreSwipe("left", frame: frame, screen: screen) {
            await runnerDo(runner, .swipe(from: path.from, to: path.to, durationMs: 200), expecting: Self.notesId, "swipe left on the note's row (the core's path)")
            await pause(1.5)
            let after = capture(notes, Self.notesId, "after-swipe", "After the runner's swipe left on the note's row (the core's path): is the note still there, which actions were revealed?")
            let still = after.map { !Self.hits($0, Self.mentions("Probe note 3317")).isEmpty } ?? false
            let band = frame
            let revealed = report(after, "buttons in the row's band (revealed swipe actions)") { e in
                e.elementType == XCUIElementTypeCode.button && e.frame.y + e.frame.height / 2 > band.y && e.frame.y + e.frame.height / 2 < band.y + band.height
                    && e.frame.width < screen.width * 0.6
            }
            let revealedText = Self.briefs(revealed)
            fact("Notes: runner swipe left on the row (\(Self.pointText(path.from)) -> \(Self.pointText(path.to)), row \(Self.rectText(frame))): note still in the list \(Self.yesNo(still)); revealed: \(revealedText)")
            if !revealed.isEmpty, let back = Self.coreSwipe("right", frame: frame, screen: screen) {
                await runnerDo(runner, .swipe(from: back.from, to: back.to, durationMs: 200), expecting: Self.notesId, "swipe right to close the actions")
                await pause(1)
            }
        }
        let look2 = await runnerLook("Notes' list before the full-width swipe", "list-before-full-swipe", runner: runner)
        let frame2 = look2?.root.flatMap { Self.rowFrame($0, "Probe note 3317", screen) }
        var stillAfterFull = false
        if let frame2 {
            let y = frame2.y + frame2.height / 2
            let from = Point(x: screen.width * 0.96, y: y)
            let to = Point(x: screen.width * 0.04, y: y)
            await runnerDo(runner, .swipe(from: from, to: to, durationMs: 200), expecting: Self.notesId, "full-width swipe left on the note's row (safe-area edge to edge)")
            await pause(2)
            let after = capture(notes, Self.notesId, "after-full-swipe", "After the runner's full-width swipe left on the note's row: was the note deleted?")
            stillAfterFull = after.map { !Self.hits($0, Self.mentions("Probe note 3317")).isEmpty } ?? false
            report(after, "alerts or sheets after the full swipe", Self.modalish)
            fact("Notes: runner full-width swipe left (\(Self.pointText(from)) -> \(Self.pointText(to))): note still in the list \(Self.yesNo(stillAfterFull))")
        } else {
            fact("Notes: the note's row is not in the runner's look before the full-width swipe (already gone: \(Self.yesNo(!existsNow(Self.node(in: notes, containing: "Probe note 3317")))))")
        }
        if stillAfterFull || existsNow(Self.node(in: notes, containing: "Probe note 3317")) {
            let cell = notes.cells.containing(NSPredicate(format: "label CONTAINS %@", "Probe note 3317")).firstMatch
            tolerant("swipe the probe note's cell left (clean-up)") { if cell.exists { cell.swipeLeft() } }
            await pause(1)
            tapFirstButton(notes, ["Delete"], "delete the probe note (clean-up)")
            await pause(1)
        }
        return nil
    }

    // MARK: - P5c: Contacts' delete confirmation (never confirmed)

    @MainActor
    func testP05c_ContactsDeleteConfirmation() async throws {
        executionTimeAllowance = 180
        begin("p5c-contacts", "Contacts' Delete Contact confirmation: what container holds it (Alert, Sheet, Popover, Other), what role and label does its destructive button have, is the form behind still in the tree? (Never confirmed.)")
        let reason = await probeContacts()
        await finish(terminating: [Self.contactsId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeContacts() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [Self.contactsId])
        let contacts = XCUIApplication(bundleIdentifier: Self.contactsId)
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        tolerant("open Contacts") { contacts.activate() }
        guard contacts.wait(for: .runningForeground, timeout: 15) else { return "Contacts did not reach the foreground" }
        await pause(2)
        for _ in 0..<2 {
            if tapFirstButton(contacts, ["Continue", "Not Now"], "close a Contacts first-launch screen") == nil { break }
            await pause(1.5)
        }
        var person = Self.node(in: contacts, labelled: "John Appleseed")
        if !existsNow(person) { person = Self.node(in: contacts, labelled: "Kate Bell") }
        guard existsNow(person) else {
            capture(contacts, Self.contactsId, "no-contact", "Why is no sample contact listed?")
            return "no sample contact (John Appleseed, Kate Bell) in Contacts"
        }
        tolerant("open the contact") { person.tap() }
        await pause(1.5)
        guard tapFirstButton(contacts, ["Edit"], "edit the contact") != nil else { return "no Edit button on the contact" }
        await pause(1.5)
        let delete = contacts.buttons["Delete Contact"].firstMatch
        var scrolls = 0
        while !hittableNow(delete), scrolls < 6 {
            drag(contacts, from: 0.8, to: 0.3, "scroll the edit form down")
            scrolls += 1
            await pause(0.8)
        }
        guard hittableNow(delete) else {
            capture(contacts, Self.contactsId, "no-delete", "Why is the Delete Contact button not reachable?")
            return "the Delete Contact button was not reached"
        }
        tolerant("tap Delete Contact (the form's button; the confirmation is never confirmed)") { delete.tap() }
        await pause(1.5)
        let tree = capture(contacts, Self.contactsId, "delete-confirmation", "Contacts' delete confirmation up: its container, its destructive button's role and label, the form behind")
        let boardTree = capture(board, Self.springboardId, "delete-confirmation-springboard", "Contacts' delete confirmation up: does SpringBoard's tree hold it?")
        let modal = report(tree, "the confirmation: modal-like nodes", Self.modalish)
        let deletes = report(tree, "nodes labelled Delete Contact (the form's and the confirmation's)") { $0.label == "Delete Contact" }
        report(tree, "nodes labelled Cancel") { $0.label == "Cancel" }
        let inBoard = boardTree.map { tree in Self.hits(tree) { $0.label == "Delete Contact" }.count } ?? 0
        var sheets = 0
        var alerts = 0
        tolerant("count sheets and alerts by query") {
            sheets = contacts.sheets.count
            alerts = contacts.alerts.count
        }
        let look = await runnerLook("Contacts' delete confirmation up", "delete-confirmation")
        let modalText = Self.typeNames(modal)
        let deleteParts: [String] = deletes.map { hit in
            let under = Self.yesNo(hit.ancestors.contains(where: Self.modalish))
            return "\(Self.brief(hit.element)) (modal-like ancestor \(under))"
        }
        let deleteText = deleteParts.joined(separator: "; ")
        let lookApp = look?.appBundleId ?? "?"
        fact("Contacts delete confirmation: modal-like nodes \(modal.count) (\(modalText)); by query sheets \(sheets), alerts \(alerts); Delete Contact nodes: \(deleteText); in SpringBoard's tree \(inBoard); the runner's look is of \(lookApp)")
        // Dismiss without confirming: the confirmation's Cancel, else a tap outside it.
        var dismissed = false
        tolerant("cancel the confirmation") {
            let sheetCancel = contacts.sheets.firstMatch.buttons["Cancel"].firstMatch
            let alertCancel = contacts.alerts.firstMatch.buttons["Cancel"].firstMatch
            if sheetCancel.exists {
                sheetCancel.tap()
                dismissed = true
            } else if alertCancel.exists {
                alertCancel.tap()
                dismissed = true
            }
        }
        if !dismissed { tapAt(contacts, 0.5, 0.06, "tap outside the confirmation") }
        await pause(1)
        var remaining = 0
        tolerant("count Delete Contact buttons") {
            remaining = contacts.buttons.matching(NSPredicate(format: "label == %@", "Delete Contact")).count
        }
        note("after dismissing: buttons labelled Delete Contact by query: \(remaining) (the form keeps one; two means the confirmation is still up)")
        return nil
    }

    // MARK: - P12a: badge, Notification Centre, Control Centre, App Switcher

    @MainActor
    func testP12a_BadgeAndSystemSurfaces() async throws {
        executionTimeAllowance = 240
        begin("p12a-surfaces", "The app-icon badge on the Home Screen (label, value); Notification Centre and Control Centre pulled down over Settings, and the App Switcher: what does the runner report as front and top layer, and what holds SpringBoard's tree?")
        await probeSurfaces()
        await finish(terminating: [])
    }

    @MainActor
    private func probeSurfaces() async {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let settings = XCUIApplication(bundleIdentifier: Self.settingsId)
        let app = launchPlayground()
        let status = app.staticTexts["probe3.system.status"].firstMatch
        if await openProbe3(app, "probe3.system") {
            let badge = app.buttons["probe3.system.badge"].firstMatch
            tolerant("set the badge") { if badge.exists { badge.tap() } }
            if let (ownerId, ownerApp) = await waitForAlert([(Self.springboardId, board)], 3) {
                let labels = readAlert(ownerApp, ownerId, "notification permission (badge)")
                capture(board, Self.springboardId, "badge-permission", "The notification permission prompt raised for the badge")
                if labels.contains("Allow") { await tapInAlert(ownerApp, "Allow", "allow badges") }
            }
            var shown = ""
            let deadline = Date().addingTimeInterval(6)
            while Date() < deadline {
                shown = labelNow(status)
                if shown.hasPrefix("badge") { break }
                await pause(0.3)
            }
            fact("badge: the app's status \"\(shown)\"")
        } else {
            fact("badge: could not produce (the System screen was not reached)")
        }
        tolerant("press Home") { XCUIDevice.shared.press(.home) }
        await pause(2)
        let home = capture(board, Self.springboardId, "home-badge", "The Home Screen with Jarvis's badge set to 7: the Jarvis icon's node (label, value, identifier)")
        let icons = report(home, "Icon nodes mentioning Jarvis") { e in
            e.elementType == XCUIElementTypeCode.icon && Self.mentions("Jarvis")(e)
        }
        var queried = "not found by query"
        tolerant("read the Jarvis icon by query") {
            let icon = board.icons["Jarvis"].firstMatch
            if icon.exists {
                let value = (icon.value as? String) ?? "\(String(describing: icon.value))"
                queried = "label \"\(icon.label)\", value \"\(value)\", id \(icon.identifier)"
            }
        }
        let iconText = Self.briefs(icons)
        fact("app-icon badge: Icon nodes for Jarvis: \(iconText); by query: \(queried)")
        await runnerLook("the Home Screen with a badge", "home-badge")

        tolerant("open Settings") { settings.activate() }
        _ = settings.wait(for: .runningForeground, timeout: 15)
        await pause(1.5)
        await surface(board, settings, slug: "notification-centre", name: "Notification Centre (pulled from the top-left edge)",
                      from: CGVector(dx: 0.15, dy: 0.003), to: CGVector(dx: 0.15, dy: 0.7), hold: false)
        await surface(board, settings, slug: "control-centre", name: "Control Centre (pulled from the top-right edge)",
                      from: CGVector(dx: 0.92, dy: 0.003), to: CGVector(dx: 0.92, dy: 0.7), hold: false)
        await surface(board, settings, slug: "app-switcher", name: "the App Switcher (swipe up from the bottom edge and hold)",
                      from: CGVector(dx: 0.5, dy: 0.998), to: CGVector(dx: 0.5, dy: 0.6), hold: true)

        tolerant("bring Jarvis back") { app.activate() }
        _ = app.wait(for: .runningForeground, timeout: 10)
        await pause(1)
        let clear = app.buttons["probe3.system.clearbadge"].firstMatch
        if hittableNow(clear) {
            tolerant("clear the badge") { clear.tap() }
            await pause(2)
            note("badge cleared: status \"\(labelNow(status))\"")
        } else {
            note("the Clear badge button is not hittable: the badge stays (Jarvis is terminated at the end)")
        }
    }

    /// Pulls a system surface over Settings with an edge drag on SpringBoard's coordinates,
    /// records SpringBoard's and Settings' trees and the runner's view, and leaves it.
    @MainActor
    private func surface(_ board: XCUIApplication, _ settings: XCUIApplication, slug: String, name: String,
                         from: CGVector, to: CGVector, hold: Bool) async {
        if settings.state != .runningForeground {
            tolerant("bring Settings back") { settings.activate() }
            _ = settings.wait(for: .runningForeground, timeout: 10)
            await pause(1)
        }
        tolerant("pull \(name)") {
            let start = board.coordinate(withNormalizedOffset: from)
            let end = board.coordinate(withNormalizedOffset: to)
            if hold {
                start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 1.0)
            } else {
                start.press(forDuration: 0.05, thenDragTo: end)
            }
        }
        await pause(1.5)
        let boardTree = capture(board, Self.springboardId, slug, "With \(name) up over Settings: what holds SpringBoard's tree?")
        let settingsTree = capture(settings, Self.settingsId, "\(slug)-settings", "With \(name) up: is Settings' full tree still reported (the app looks in front)?")
        let look = await runnerLook("\(name) up over Settings", slug)
        var named = "none"
        if let boardTree {
            let containers = Self.hits(boardTree) { e in
                !e.identifier.isEmpty && (e.elementType == XCUIElementTypeCode.other || e.elementType == XCUIElementTypeCode.window || e.elementType == XCUIElementTypeCode.scrollView)
            }
            named = Self.typeNames(Array(containers.prefix(8)))
        }
        let boardCount = boardTree.map { String($0.count) } ?? "?"
        let settingsCount = settingsTree.map { String($0.count) } ?? "?"
        let lookApp = look?.appBundleId ?? "?"
        fact("\(name): Settings state \(stateText(settings)); SpringBoard's tree \(boardCount) nodes (identified containers: \(named)); Settings' tree \(settingsCount) nodes; the runner's look is of \(lookApp)")
        tolerant("press Home to leave \(name)") { XCUIDevice.shared.press(.home) }
        await pause(1.5)
    }

    // MARK: - P12b: swipe back from the middle of content

    @MainActor
    func testP12b_SwipeBackFromContent() async throws {
        executionTimeAllowance = 120
        begin("p12b-swipe-back", "In Settings > General, does the runner's element-less swipe right (the core's path, from mid-screen, not from the edge) navigate back (iOS 26 swipe-back from anywhere)?")
        let reason = await probeSwipeBack()
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeSwipeBack() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        let settings = XCUIApplication(bundleIdentifier: Self.settingsId)
        tolerant("open Settings") { settings.activate() }
        guard settings.wait(for: .runningForeground, timeout: 15) else { return "Settings did not reach the foreground" }
        await pause(1.5)
        var general = settings.descendants(matching: .any)["com.apple.settings.general"].firstMatch
        if !existsNow(general) { general = Self.node(in: settings, labelled: "General") }
        guard existsNow(general) else { return "no General row in Settings" }
        tolerant("open General") { general.tap() }
        let bar = settings.navigationBars["General"].firstMatch
        guard waitFor(bar, "the General navigation bar", 6) else { return "General did not open" }
        await pause(1)
        capture(settings, Self.settingsId, "general", "Settings > General before the swipe")
        let runner = XCUITestPhoneRunner()
        let look = await runnerLook("Settings > General", "general", runner: runner)
        guard let screen = look?.screenSize, let path = Self.coreSwipe("right", frame: nil, screen: screen) else {
            fact("swipe back: could not produce (no runner look)")
            return nil
        }
        await runnerDo(runner, .swipe(from: path.from, to: path.to, durationMs: 200), expecting: Self.settingsId, "element-less swipe right (the core's path)")
        await pause(1.5)
        let back = !existsNow(bar)
        capture(settings, Self.settingsId, "after-swipe", "Settings after the runner's element-less swipe right from mid-screen: back on the Settings root?")
        await runnerLook("Settings after the swipe right", "after-swipe")
        fact("swipe back: the runner's swipe right \(Self.pointText(path.from)) -> \(Self.pointText(path.to)) (starts \(Int(path.from.x)) pt from the left edge) navigated back from General: \(Self.yesNo(back))")
        return nil
    }
}
