import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Probe round 3 on system apps and web content (the audit's P5, P6, P7, P12, P15): Safari and
/// an in-app `WKWebView` on local pages (`Probe3Server`, loopback only), Messages' compose
/// screen reached through `sms:` (never sent), a Reminders row under the runner's own swipes
/// (Notes is not on the iOS 26.5 simulator), Contacts' delete confirmation (never confirmed),
/// the app-icon badge, Notification Centre, Control Centre, the App Switcher, swipe-back in
/// Settings, and SpringBoard's own states (Today view, App Library, a folder, jiggle mode).
/// Conventions: `Probe3Case`; every system app is opened through `bringForward`.
///
/// Simulator time (typical): P5a ~60 s, P5b ~80 s, P5c ~70 s, P6a ~75 s, P6b ~70 s, P6c ~40 s,
/// P7 ~40 s, P12a ~85 s, P12b ~30 s, P15 ~90 s.
final class TreeFactProbe3SystemTests: Probe3Case {
    private static let messagesId = "com.apple.MobileSMS"
    private static let remindersId = "com.apple.reminders"
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
    /// or OK. Run 139: Safari draws it as `Other id=SFDialogView` inside `WebView id=WebView`
    /// in its own tree (plain Buttons, a TextField for prompt()), with no Alert node anywhere;
    /// an Alert (in the app or SpringBoard) is still looked for.
    @MainActor
    private func jsDialog(_ app: XCUIApplication, _ board: XCUIApplication, button: String, slug: String, text: String, appId: String) async {
        let trigger = app.buttons[button].firstMatch
        guard waitFor(trigger, "the \(button) button", 5) else {
            fact("\(button): could not produce (no button on the page)")
            return
        }
        tolerant("tap \(button)") { trigger.tap() }
        let (holder, owner) = await waitForDialog(app, appId, board, 5)
        await pause(0.5)
        let tree = capture(app, appId, slug, "After \(button): does \(appId)'s tree hold the JavaScript dialog (SFDialogView?), with what roles?")
        let boardTree = capture(board, Self.springboardId, "\(slug)-springboard", "After \(button): does SpringBoard's tree hold it?")
        let inApp = report(tree, "\(button): nodes mentioning its text (\(appId))", Self.mentions(text))
        let inBoard = report(boardTree, "\(button): nodes mentioning its text (SpringBoard)", Self.mentions(text))
        let modal = report(tree, "\(button): modal-like nodes (\(appId))", Self.modalish)
        if slug == "js-prompt" {
            report(tree, "\(button): TextField nodes") { $0.elementType == XCUIElementTypeCode.textField }
        }
        let dialogButtons = sfDialogFacts(tree, button)
        let look = await runnerLook("the \(button) dialog up", slug)
        typedDialogFacts(look, button)
        var labels: [String] = []
        if let (ownerId, ownerApp) = owner { labels = readAlert(ownerApp, ownerId, button) }
        let textHolder = inApp.first.map { hit in "\(Self.brief(hit.element)), ancestors \(Self.ancestry(hit, 4))" } ?? "none"
        let modalText = Self.typeNames(modal)
        let lookApp = look?.appBundleId ?? "?"
        fact("\(button): held by \(holder); its text in \(appId)'s snapshot: \(inApp.count) node(s) (first: \(textHolder)); in SpringBoard's: \(inBoard.count); modal-like nodes in \(appId)'s: \(modalText); Alert buttons by query \(labels); the runner's top layer \(lastRunnerFront), its look of \(lookApp)")
        let prefer = ["Cancel", "Close", "OK"]
        let viaDialog = await dismissSFDialog(app, dialogButtons, prefer: prefer, "dismiss the \(button) dialog through its SFDialogView")
        if !viaDialog {
            if let (_, ownerApp) = owner, let choice = prefer.first(where: { labels.contains($0) }) {
                await tapInAlert(ownerApp, choice, "dismiss the \(button) dialog")
            } else {
                tapFirstButton(app, prefer, "dismiss the \(button) dialog")
            }
        }
        await pause(0.8)
        note("\(button): the page's result line is now \"\(labelNow(Self.node(in: app, containing: "result:")))\"")
    }

    /// Waits up to `timeout` s for Safari's own dialog (`SFDialogView` in `app`) or an Alert in
    /// `app` or SpringBoard. Returns what holds it and, for an Alert, its owner.
    @MainActor
    private func waitForDialog(_ app: XCUIApplication, _ appId: String, _ board: XCUIApplication, _ timeout: TimeInterval) async -> (String, (String, XCUIApplication)?) {
        let dialog = app.descendants(matching: .any).matching(identifier: "SFDialogView").firstMatch
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if existsNow(dialog) { return ("an SFDialogView in \(appId)'s tree", nil) }
            for (id, candidate) in [(appId, app), (Self.springboardId, board)] {
                var up = false
                tolerant("look for an alert in \(id)") { up = candidate.alerts.firstMatch.exists }
                if up { return ("an Alert in \(id)", (id, candidate)) }
            }
            await pause(0.3)
        }
        return ("neither an SFDialogView nor an Alert within \(timeout.compactText) s", nil)
    }

    /// The `SFDialogView` node in `tree`: where it sits, its children, buttons and fields (as
    /// a fact). Returns its buttons' labels.
    @MainActor
    @discardableResult
    private func sfDialogFacts(_ tree: RawElement?, _ what: String) -> [String] {
        guard let tree else {
            fact("\(what): SFDialogView: no tree (the capture failed)")
            return []
        }
        guard let dialog = Self.hits(tree, { $0.identifier == "SFDialogView" }).first else {
            fact("\(what): no SFDialogView node in the tree")
            return []
        }
        let underWebView = dialog.ancestors.contains { $0.elementType == XCUIElementTypeCode.webView }
        let children: [String] = dialog.element.children.map { "\(Self.brief($0)) \(Self.rectText($0.frame))" }
        let buttons = Self.hits(dialog.element) { $0.elementType == XCUIElementTypeCode.button }
        let fields = Self.hits(dialog.element, Self.isType([XCUIElementTypeCode.textField, XCUIElementTypeCode.secureTextField]))
        let fieldParts: [String] = fields.map { hit in "\(Self.brief(hit.element)) placeholder \"\(hit.element.placeholder ?? "")\"" }
        let texts = Self.hits(dialog.element) { $0.elementType == XCUIElementTypeCode.textView && !$0.label.isEmpty }
        let textParts: [String] = texts.map { "\"\(Self.clip($0.element.label))\"" }
        let fieldText = fieldParts.isEmpty ? "none" : fieldParts.joined(separator: "; ")
        let textText = textParts.isEmpty ? "none" : textParts.joined(separator: ", ")
        fact("\(what): SFDialogView \(Self.rectText(dialog.element.frame)), WebView ancestor \(Self.yesNo(underWebView)), ancestors \(Self.ancestry(dialog, 5)); its children: \(children.joined(separator: "; ")); its buttons: \(Self.briefs(buttons)); its fields: \(fieldText); its texts: \(textText)")
        return buttons.map { $0.element.label }
    }

    /// The same dialog in the runner's typed tree (what the core receives): the node with
    /// identifier SFDialogView, its role, and the roles of the buttons and fields under it.
    @MainActor
    private func typedDialogFacts(_ look: RunnerObservation?, _ what: String) {
        guard let root = look?.root else {
            fact("\(what): the runner's typed tree: none")
            return
        }
        guard let node = Self.findAll(root, { ($0.identifier ?? "") == "SFDialogView" }).first else {
            let modal = Self.findAll(root) { $0.role == .alert || $0.role == .sheet }.count
            fact("\(what): the runner's typed tree has no SFDialogView node (typed Alert/Sheet nodes: \(modal))")
            return
        }
        let inside: [String] = Self.findAll(node) { $0.role == .button || $0.role == .textField }.map { Self.describeNode($0) }
        let insideText = inside.isEmpty ? "no button or field" : inside.joined(separator: "; ")
        fact("\(what): the runner's typed tree: \(Self.describeNode(node)); under it: \(insideText)")
    }

    /// Taps the first of `prefer` among `buttons` inside the SFDialogView and waits (≤ 4 s)
    /// for it to go. False when none of them is there.
    @MainActor
    private func dismissSFDialog(_ app: XCUIApplication, _ buttons: [String], prefer: [String], _ what: String) async -> Bool {
        guard let choice = prefer.first(where: { buttons.contains($0) }) else { return false }
        let dialog = app.descendants(matching: .any).matching(identifier: "SFDialogView").firstMatch
        tolerant(what) {
            let button = dialog.buttons[choice].firstMatch
            if button.exists { button.tap() } else { note("\(what): no button \"\(choice)\" in the SFDialogView by query") }
        }
        let deadline = Date().addingTimeInterval(4)
        while existsNow(dialog), Date() < deadline { await pause(0.3) }
        note("\(what): tapped \"\(choice)\"; SFDialogView still up afterwards: \(Self.yesNo(existsNow(dialog)))")
        return true
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
        await pause(1)
        let (holder, owner) = await waitForDialog(safari, Self.safariId, board, 6)
        await pause(0.5)
        let tree = capture(safari, Self.safariId, "basic-auth", "An HTTP Basic auth challenge in Safari: is the sign-in prompt in Safari's tree (SFDialogView?), as what role, with what fields and buttons?")
        let boardTree = capture(board, Self.springboardId, "basic-auth-springboard", "An HTTP Basic auth challenge: does SpringBoard's tree hold the prompt?")
        let modal = report(tree, "Basic auth: modal-like nodes (Safari)", Self.modalish)
        let fields = report(tree, "Basic auth: TextField and SecureTextField nodes (Safari)", Self.isType([XCUIElementTypeCode.textField, XCUIElementTypeCode.secureTextField]))
        let named = report(tree, "Basic auth: nodes naming the realm, the host or Log In", limit: 12) { e in
            Self.mentions("Probe realm")(e) || Self.mentions("127.0.0.1")(e) || Self.mentions("Log In")(e) || Self.mentions("Sign In")(e)
        }
        let inBoard = boardTree.map { Self.hits($0, Self.modalish).count } ?? 0
        let dialogButtons = sfDialogFacts(tree, "HTTP Basic auth")
        let look = await runnerLook("the Basic auth prompt up", "basic-auth")
        typedDialogFacts(look, "HTTP Basic auth")
        var labels: [String] = []
        if let (ownerId, ownerApp) = owner { labels = readAlert(ownerApp, ownerId, "Basic auth") }
        let modalText = Self.typeNames(modal)
        let fieldParts: [String] = fields.map { hit in "\(Self.brief(hit.element)) placeholder \"\(hit.element.placeholder ?? "")\"" }
        let fieldText = fieldParts.isEmpty ? "none" : fieldParts.joined(separator: "; ")
        let lookApp = look?.appBundleId ?? "?"
        fact("HTTP Basic auth: held by \(holder); Safari's modal-like nodes: \(modalText); fields: \(fieldText); nodes naming realm/host/Log In/Sign In: \(named.count); SpringBoard modal-like nodes \(inBoard); Alert buttons by query \(labels); the runner's top layer \(lastRunnerFront), its look of \(lookApp)")
        let viaDialog = await dismissSFDialog(safari, dialogButtons, prefer: ["Cancel"], "cancel the sign-in through its SFDialogView")
        if !viaDialog {
            if let (_, ownerApp) = owner, labels.contains("Cancel") {
                await tapInAlert(ownerApp, "Cancel", "cancel the sign-in")
            } else {
                tapFirstButton(safari, ["Cancel", "Close"], "cancel the sign-in")
            }
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
        begin("p5a-messages", "Where does the runner's openUrl(sms:+15555550123) land, and what does the runner report there? In Messages' compose screen with a body typed: what are the send button's role, label and identifier, and what is the Return key? (Nothing is sent; the draft is discarded.)")
        let reason = await probeMessages()
        await finish(terminating: [Self.messagesId])
        if let reason { throw XCTSkip(reason) }
    }

    /// Run 139: Messages' list has no compose button in its tree (NavigationBar "Messages",
    /// conversation Cells, a Toolbar with a SearchField and Dictate), so the compose screen is
    /// reached through the URL the agent itself would use.
    @MainActor
    private func probeMessages() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        guard iconInstalled("Messages") else {
            unavailableApps.insert(Self.messagesId)
            fact("Messages: no Messages icon in SpringBoard's tree: not opened")
            return "Messages is not on this simulator's Home Screen"
        }
        let messages = XCUIApplication(bundleIdentifier: Self.messagesId)
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let runner = XCUITestPhoneRunner()
        checkpoint("the runner's openUrl(sms:+15555550123) (bounded at 10 s)", force: true)
        await runnerDo(runner, .openUrl(url: "sms:+15555550123"), expecting: nil, "open sms:+15555550123 (the runner's openUrl)")
        var up = false
        tolerant("wait for Messages in the foreground") { up = messages.wait(for: .runningForeground, timeout: 10) }
        guard up else {
            if !isRunning(messages) { unavailableApps.insert(Self.messagesId) }
            capture(board, Self.springboardId, "after-open-springboard", "After openUrl(sms:…): Messages is not in front within 10 s; what does SpringBoard show (a prompt, the Home Screen)?")
            await runnerLook("after openUrl(sms:…), Messages not in front", "after-open", runner: runner)
            fact("sms: URL: Messages not in the foreground within 10 s (state \(stateText(messages))); the runner's top layer \(lastRunnerFront)")
            return "Messages did not come forward within 10 s of opening sms:+15555550123"
        }
        await pause(2)
        for _ in 0..<2 {
            if tapFirstButton(messages, ["Continue", "Not Now"], "close a Messages first-launch screen") == nil { break }
            await pause(1.5)
        }
        let landed = capture(messages, Self.messagesId, "compose-opened", "Where did sms:+15555550123 land: a new-message screen with the recipient filled in? Its bars, recipient and body nodes; is a keyboard up?")
        let look = await runnerLook("Messages after openUrl(sms:+15555550123)", "compose-opened", runner: runner)
        let recipients = report(landed, "recipient nodes (mentioning 555)", Self.mentions("555"))
        let inputs = report(landed, "text inputs (TextField, TextView, SearchField)", Self.isType([XCUIElementTypeCode.textField, XCUIElementTypeCode.textView, XCUIElementTypeCode.searchField]))
        let bars = report(landed, "NavigationBar nodes", Self.isType([XCUIElementTypeCode.navigationBar]))
        let keyboardUp = landed.map { !Self.hits($0) { $0.elementType == XCUIElementTypeCode.keyboard }.isEmpty } ?? false
        let recipientText = Self.briefs(Array(recipients.prefix(3)))
        let inputText = Self.briefs(Array(inputs.prefix(4)))
        let barText = Self.typeNames(bars)
        fact("sms: URL lands in Messages (state \(stateText(messages))): navigation bars \(barText); recipient nodes \(recipientText); text inputs \(inputText); keyboard up \(Self.yesNo(keyboardUp)); the runner's top layer \(lastRunnerFront), its look of \(look?.appBundleId ?? "?")")

        let predicate = NSPredicate(format: "label CONTAINS[c] %@ OR placeholderValue CONTAINS[c] %@ OR identifier CONTAINS[c] %@", "message", "message", "message")
        let bodyView = messages.textViews.matching(predicate).firstMatch
        let body = existsNow(bodyView) ? bodyView : messages.textFields.matching(predicate).firstMatch
        if existsNow(body) {
            tolerant("focus the message body") { body.tap() }
            await pause(1)
        } else {
            note("no message body field by query (label, placeholder or identifier containing message)")
        }
        var keyboard = false
        tolerant("wait for the keyboard") { keyboard = messages.keyboards.firstMatch.waitForExistence(timeout: 3) }
        guard keyboard else {
            capture(messages, Self.messagesId, "no-keyboard", "Why is there no keyboard for the message body?")
            fact("Messages compose: no keyboard after focusing the body: nothing typed")
            tapFirstButton(messages, ["Cancel", "Close"], "close the new message")
            return "no keyboard for Messages' message body"
        }
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
        let filled = await runnerLook("Messages' compose screen filled in", "compose-filled", runner: runner)
        let typedSend = filled?.root.map { root in Self.findAll(root) { ($0.label ?? "").localizedCaseInsensitiveContains("send") || ($0.identifier ?? "").localizedCaseInsensitiveContains("send") }.map { Self.describeNode($0) } } ?? []
        let sendText = Self.briefs(send)
        let typedText = typedSend.isEmpty ? "none" : typedSend.joined(separator: "; ")
        fact("Messages compose: send controls in the raw tree: \(sendText); in the runner's typed tree: \(typedText)")
        let keyText = key.map { Self.brief($0) } ?? "not located"
        fact("Messages compose: the Return key (bottom-right key of the Keyboard node) is \(keyText)")
        // Discard: delete the body (no draft is kept), then cancel. Return is never pressed and
        // the send control never touched.
        tolerant("delete the body text") { messages.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 18)) }
        await pause(0.5)
        tapFirstButton(messages, ["Cancel", "Close"], "cancel the new message")
        await pause(1.5)
        if existsNow(messages.buttons["Delete Draft"].firstMatch) {
            tapFirstButton(messages, ["Delete Draft"], "delete the draft")
        }
        let left = existsNow(Self.node(in: messages, containing: "Probe body"))
        note("after discarding: \"Probe body\" still in Messages' tree by query: \(Self.yesNo(left))")
        return nil
    }

    // MARK: - P5b: a Reminders row under the runner's swipes

    @MainActor
    func testP05b_RemindersRowSwipe() async throws {
        executionTimeAllowance = 180
        begin("p5b-reminders", "A reminder's row in Reminders (Notes is not on the iOS 26.5 simulator): what is the row in the tree; does the runner's own swipe (the core's path for a swipe left on the row, then a full-width swipe) reveal swipe actions (roles, labels, identifiers) or delete the reminder?")
        let reason = await probeReminders()
        await finish(terminating: [Self.remindersId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeReminders() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        guard let reminders = await bringForward(Self.remindersId, icon: "Reminders") else {
            return "Reminders could not come forward within 10 s on this simulator"
        }
        await pause(2)
        for _ in 0..<3 {
            if tapFirstButton(reminders, ["Continue", "Not Now", "Get Started", "OK"], "close a Reminders first-launch screen") == nil { break }
            await pause(1.5)
        }
        capture(reminders, Self.remindersId, "start", "Reminders' first screen: where is the New Reminder button?")
        let create = reminders.buttons.matching(NSPredicate(format: "label CONTAINS[c] %@ OR identifier CONTAINS[c] %@", "New Reminder", "newreminder")).firstMatch
        guard waitFor(create, "the New Reminder button", 5) else { return "no New Reminder button in Reminders (its first screen is in the tree file)" }
        tolerant("tap New Reminder") { create.tap() }
        var keyboard = false
        tolerant("wait for the keyboard") { keyboard = reminders.keyboards.firstMatch.waitForExistence(timeout: 4) }
        guard keyboard else {
            capture(reminders, Self.remindersId, "no-keyboard", "After New Reminder: why is there no keyboard?")
            return "no keyboard after New Reminder: the reminder could not be typed"
        }
        tolerant("type the reminder") { reminders.typeText("Probe reminder 3317") }
        await pause(1)
        capture(reminders, Self.remindersId, "editor", "The new reminder being edited")
        // Inside a list the new row ends with Done; from the lists overview New Reminder opens
        // a sheet that ends with Add.
        if tapFirstButton(reminders, ["Done", "Add"], "finish the reminder") == nil {
            tapAt(reminders, 0.5, 0.75, "tap below the reminders to end editing")
        }
        await pause(1.5)
        if reminderBand(reminders) == nil {
            // Back on the lists overview: open the list that holds it (the default list, else All).
            for prefix in ["Reminders", "All"] {
                let list = reminders.cells.matching(NSPredicate(format: "label BEGINSWITH %@", prefix)).firstMatch
                if hittableNow(list) {
                    tolerant("open the list \(prefix)") { list.tap() }
                    await pause(1.5)
                    break
                }
            }
            note("the probe reminder listed after opening a list: \(Self.yesNo(reminderBand(reminders) != nil))")
        }
        let list = capture(reminders, Self.remindersId, "list", "Reminders' list with the new reminder: what is its row (role, label, identifier, frame)?")
        let row = list.flatMap { tree in
            Self.hits(tree) { $0.elementType == XCUIElementTypeCode.cell && !Self.hits($0, Self.mentions("Probe reminder 3317")).isEmpty }.last
        }
        guard let row else {
            let anywhere = list.map { !Self.hits($0, Self.mentions("Probe reminder 3317")).isEmpty } ?? false
            fact("Reminders: no Cell holds the new reminder (its text anywhere in the tree: \(Self.yesNo(anywhere)))")
            await cleanUpReminder(reminders)
            return "the new reminder's row was not found in Reminders' list"
        }
        let rowParts = Self.hits(row.element) { !$0.label.isEmpty || ($0.value ?? "").contains("Probe") }
        let rowText = Self.briefs(Array(rowParts.prefix(6)))
        fact("Reminders: the reminder's row: \(Self.brief(row.element)) \(Self.rectText(row.element.frame)); inside: \(rowText)")
        let runner = XCUITestPhoneRunner()
        let look = await runnerLook("Reminders' list with the new reminder", "list", runner: runner)
        guard let screen = look?.screenSize else {
            fact("Reminders swipes: could not produce (no runner look)")
            await cleanUpReminder(reminders)
            return nil
        }
        let frame = look?.root.flatMap { Self.rowFrame($0, "Probe reminder 3317", screen) } ?? row.element.frame
        if let path = Self.coreSwipe("left", frame: frame, screen: screen) {
            await runnerDo(runner, .swipe(from: path.from, to: path.to, durationMs: 200), expecting: Self.remindersId, "swipe left on the reminder's row (the core's path)")
            await pause(1.5)
            let after = capture(reminders, Self.remindersId, "after-swipe", "After the runner's swipe left on the reminder's row (the core's path): is the reminder still there, which actions were revealed (role, label, identifier)?")
            let still = after.map { !Self.hits($0, Self.mentions("Probe reminder 3317")).isEmpty } ?? false
            let band = frame
            let revealed = report(after, "buttons in the row's band (revealed swipe actions)") { e in
                e.elementType == XCUIElementTypeCode.button && e.frame.y + e.frame.height / 2 > band.y && e.frame.y + e.frame.height / 2 < band.y + band.height
                    && e.frame.width < screen.width * 0.6
            }
            let revealedText = Self.briefs(revealed)
            fact("Reminders: runner swipe left on the row (\(Self.pointText(path.from)) -> \(Self.pointText(path.to)), row \(Self.rectText(frame))): reminder still in the list \(Self.yesNo(still)); buttons in the row's band: \(revealedText)")
            if still, let back = Self.coreSwipe("right", frame: frame, screen: screen) {
                await runnerDo(runner, .swipe(from: back.from, to: back.to, durationMs: 200), expecting: Self.remindersId, "swipe right to close the actions")
                await pause(1)
            }
        }
        let look2 = await runnerLook("Reminders' list before the full-width swipe", "list-before-full-swipe", runner: runner)
        if let frame2 = look2?.root.flatMap({ Self.rowFrame($0, "Probe reminder 3317", screen) }) {
            let y = frame2.y + frame2.height / 2
            let from = Point(x: screen.width * 0.96, y: y)
            let to = Point(x: screen.width * 0.04, y: y)
            await runnerDo(runner, .swipe(from: from, to: to, durationMs: 200), expecting: Self.remindersId, "full-width swipe left on the reminder's row (safe-area edge to edge)")
            await pause(2)
            let after = capture(reminders, Self.remindersId, "after-full-swipe", "After the runner's full-width swipe left on the reminder's row: was the reminder deleted, or its actions revealed?")
            let still = after.map { !Self.hits($0, Self.mentions("Probe reminder 3317")).isEmpty } ?? false
            report(after, "alerts or sheets after the full swipe", Self.modalish)
            let revealed = report(after, "buttons in the row's band after the full swipe") { e in
                e.elementType == XCUIElementTypeCode.button && abs(e.frame.y + e.frame.height / 2 - y) < frame2.height / 2 && e.frame.width < screen.width * 0.6
            }
            fact("Reminders: runner full-width swipe left (\(Self.pointText(from)) -> \(Self.pointText(to))): reminder still in the list \(Self.yesNo(still)); buttons in the row's band: \(Self.briefs(revealed))")
        } else {
            fact("Reminders: the reminder's row is not in the runner's look before the full-width swipe (already gone: \(Self.yesNo(reminderBand(reminders) == nil)))")
        }
        await cleanUpReminder(reminders)
        return nil
    }

    /// Deletes the probe reminder if it is still listed (XCUITest's own swipe on its cell, then
    /// Delete), and records whether it is gone.
    @MainActor
    private func cleanUpReminder(_ reminders: XCUIApplication) async {
        // The title may be a TextView's value rather than a label, so the row is found in the
        // snapshot (label, value or identifier), never by a predicate on `value`.
        guard let band = reminderBand(reminders) else {
            note("clean-up: the probe reminder is not listed")
            return
        }
        tolerant("swipe the probe reminder's row left, edge to edge (clean-up)") {
            let origin = reminders.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            let y = band.y + band.height / 2
            let start = origin.withOffset(CGVector(dx: band.x + band.width - 8, dy: y))
            let end = origin.withOffset(CGVector(dx: band.x + 8, dy: y))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        await pause(1.5)
        if reminderBand(reminders) != nil {
            tapFirstButton(reminders, ["Delete"], "delete the probe reminder (clean-up)")
            await pause(1.5)
        }
        fact("Reminders clean-up: the probe reminder still listed afterwards: \(Self.yesNo(reminderBand(reminders) != nil))")
    }

    /// The frame of the Cell (else the node) holding "Probe reminder 3317" in Reminders'
    /// snapshot, or nil.
    @MainActor
    private func reminderBand(_ reminders: XCUIApplication) -> Rect? {
        var raw: RawElement?
        tolerant("snapshot Reminders") { raw = SnapshotCopy.copy(try reminders.snapshot()) }
        guard let raw else { return nil }
        let mention = Self.mentions("Probe reminder 3317")
        if let cell = Self.hits(raw, { $0.elementType == XCUIElementTypeCode.cell && !Self.hits($0, mention).isEmpty }).last {
            return cell.element.frame
        }
        return Self.hits(raw, mention).first?.element.frame
    }

    // MARK: - P5c: Contacts' delete confirmation (never confirmed)

    @MainActor
    func testP05c_ContactsDeleteConfirmation() async throws {
        executionTimeAllowance = 180
        begin("p5c-contacts", "Contacts' Delete Contact confirmation: what container holds it (Alert, Sheet, Popover, Other), what role and label does its destructive button have, is the card behind still in the tree, what does the runner report? (Never confirmed.)")
        let reason = await probeContacts()
        await finish(terminating: [Self.contactsId])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeContacts() async -> String? {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        guard let contacts = await bringForward(Self.contactsId, icon: "Contacts") else {
            return "Contacts could not come forward within 10 s on this simulator"
        }
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        await pause(2)
        for _ in 0..<2 {
            if tapFirstButton(contacts, ["Continue", "Not Now"], "close a Contacts first-launch screen") == nil { break }
            await pause(1.5)
        }
        // Run 147: two 60 s XCUITest idle waits on Contacts' edit form ("App animations
        // complete notification not received") ran the test out of time. From here on, every
        // frame comes from a snapshot and every tap and drag is synthesized through
        // SpringBoard's coordinates, so no call waits for Contacts to idle.
        guard let list = snapshotTree(contacts, "Contacts' list") else { return "no snapshot of Contacts' list" }
        var name = "John Appleseed"
        var personFrame = Self.hits(list) { $0.label == name }.first?.element.frame
        if personFrame == nil {
            name = "Kate Bell"
            personFrame = Self.hits(list) { $0.label == name }.first?.element.frame
        }
        guard let person = personFrame else {
            capture(contacts, Self.contactsId, "no-contact", "Why is no sample contact listed?")
            return "no sample contact (John Appleseed, Kate Bell) in Contacts"
        }
        boardTap(person, "open the contact \(name)")
        await pause(2)
        guard let card = snapshotTree(contacts, "the contact's card"),
              let edit = Self.hits(card, { $0.elementType == XCUIElementTypeCode.button && $0.label == "Edit" }).first?.element.frame else {
            capture(contacts, Self.contactsId, "no-edit", "Why is there no Edit button on the contact's card?")
            return "no Edit button on the contact"
        }
        boardTap(edit, "edit the contact")
        await pause(2)
        // Run 139: "Delete Contact" is a Cell (with a StaticText), not a Button.
        var found: Rect?
        var screen = Rect(x: 0, y: 0, width: 0, height: 0)
        var scrolls = 0
        while scrolls <= 6 {
            guard let form = snapshotTree(contacts, "the edit form") else { break }
            screen = form.frame
            let hits = Self.hits(form) { $0.label == "Delete Contact" && ($0.elementType == XCUIElementTypeCode.cell || $0.elementType == XCUIElementTypeCode.staticText) }
            if let hit = hits.first(where: { h in
                let mid = h.element.frame.y + h.element.frame.height / 2
                return mid > screen.height * 0.15 && mid < screen.height * 0.85
            }) {
                found = hit.element.frame
                break
            }
            if scrolls == 6 { break }
            boardScroll(from: 0.8, to: 0.3, "scroll the edit form down (through SpringBoard)")
            scrolls += 1
            await pause(1)
        }
        guard let target = found else {
            capture(contacts, Self.contactsId, "no-delete", "Why is the Delete Contact cell not reachable?")
            return "the Delete Contact cell was not reached within 6 scrolls"
        }
        note("the Delete Contact cell is in the middle band at \(Self.rectText(target)) after \(scrolls) scroll(s)")
        boardTap(target, "tap the Delete Contact cell (the form's; the confirmation is never confirmed)")
        await pause(1.5)
        let tree = capture(contacts, Self.contactsId, "delete-confirmation", "Contacts' delete confirmation up: its container (Alert, Sheet, Popover, Other), its buttons' roles and labels, the card behind")
        let boardTree = capture(board, Self.springboardId, "delete-confirmation-springboard", "Contacts' delete confirmation up: does SpringBoard's tree hold it?")
        let modal = report(tree, "the confirmation: modal-like nodes", Self.modalish)
        let deletes = report(tree, "nodes labelled Delete Contact (the form's cell and the confirmation's button)") { $0.label == "Delete Contact" }
        let cancels = report(tree, "nodes labelled Cancel") { $0.label == "Cancel" }
        let cardNodes = report(tree, "the card behind: nodes naming the contact or its edit form (\(name), add phone, Ringtone)", limit: 6) { e in
            e.label == name || e.label == "add phone" || e.label == "Ringtone"
        }
        let inBoard = boardTree.map { tree in Self.hits(tree) { $0.label == "Delete Contact" }.count } ?? 0
        let sheets = tree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.sheet }.count } ?? 0
        let alerts = tree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.alert }.count } ?? 0
        let look = await runnerLook("Contacts' delete confirmation up", "delete-confirmation")
        let typedModal = look?.root.map { root in Self.findAll(root) { $0.role == .alert || $0.role == .sheet }.map { Self.describeNode($0) } } ?? []
        let typedDeletes = look?.root.map { root in Self.findAll(root) { ($0.label ?? "") == "Delete Contact" }.map { Self.describeNode($0) } } ?? []
        let modalText = Self.typeNames(modal)
        let deleteParts: [String] = deletes.map { hit in
            let under = Self.yesNo(hit.ancestors.contains(where: Self.modalish))
            return "\(Self.brief(hit.element)) \(Self.rectText(hit.element.frame)) (modal-like ancestor \(under); ancestors \(Self.ancestry(hit, 4)))"
        }
        let deleteText = deleteParts.joined(separator: "; ")
        let cardParts: [String] = cardNodes.map { hit in "\(Self.brief(hit.element)) on screen \(Self.yesNo(Self.onScreen(hit.element.frame, tree?.frame ?? hit.element.frame)))" }
        let cardText = cardParts.isEmpty ? "not in the tree" : cardParts.joined(separator: "; ")
        let lookApp = look?.appBundleId ?? "?"
        let typedModalText = typedModal.isEmpty ? "none" : typedModal.joined(separator: "; ")
        let typedDeleteText = typedDeletes.isEmpty ? "none" : typedDeletes.joined(separator: "; ")
        fact("Contacts delete confirmation: modal-like nodes \(modal.count) (\(modalText)); in the snapshot Sheet nodes \(sheets), Alert nodes \(alerts); Delete Contact nodes: \(deleteText); in SpringBoard's tree \(inBoard)")
        fact("Contacts delete confirmation: the card behind: \(cardText)")
        fact("Contacts delete confirmation: the runner's top layer \(lastRunnerFront), its look of \(lookApp); typed Alert/Sheet nodes: \(typedModalText); typed Delete Contact nodes: \(typedDeleteText)")
        // Dismiss without confirming: a Cancel inside the confirmation (else any Cancel), else
        // a tap outside, all through SpringBoard. Delete Contact is never tapped again.
        let insideCancel = cancels.first { hit in hit.ancestors.contains(where: Self.modalish) && hit.element.elementType == XCUIElementTypeCode.button }
        let anyCancel = cancels.first { $0.element.elementType == XCUIElementTypeCode.button }
        var dismissed = "a tap outside"
        if let cancel = insideCancel ?? anyCancel {
            boardTap(cancel.element.frame, "cancel the confirmation")
            dismissed = insideCancel != nil ? "the confirmation's Cancel" : "a Cancel button"
        } else {
            boardTap(Rect(x: screen.width * 0.5 - 1, y: screen.height * 0.06 - 1, width: 2, height: 2), "tap outside the confirmation")
        }
        await pause(1.5)
        let afterTree = snapshotTree(contacts, "Contacts after dismissing")
        let remaining = afterTree.map { Self.hits($0) { $0.label == "Delete Contact" }.count } ?? -1
        let formUp = afterTree.map { !Self.hits($0) { $0.elementType == XCUIElementTypeCode.cell && $0.label == "Delete Contact" }.isEmpty } ?? false
        fact("Contacts delete confirmation: dismissed with \(dismissed); nodes labelled Delete Contact in the snapshot afterwards: \(remaining) (the form keeps its cell and text; more means the confirmation is still up); the edit form still up (its Delete Contact cell): \(Self.yesNo(formUp))")
        return nil
    }

    /// A tap at the centre of `frame` synthesized through SpringBoard's coordinates (no wait
    /// for the app under it to idle).
    @MainActor
    private func boardTap(_ frame: Rect, _ what: String) {
        tapThroughSpringBoard(CGRect(x: frame.x, y: frame.y, width: frame.width, height: frame.height), what)
    }

    /// A vertical drag at mid-width between two normalised heights, synthesized through
    /// SpringBoard's coordinates.
    @MainActor
    private func boardScroll(from: Double, to: Double, _ what: String) {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        tolerant(what) {
            let start = board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: from))
            let end = board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: to))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        note("\(what): dragged from \(Int(from * 100))% to \(Int(to * 100))% of the height")
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

        if await bringForward(Self.settingsId, icon: "Settings", fresh: false) != nil {
            await pause(1.5)
            await surface(board, settings, slug: "notification-centre", name: "Notification Centre (pulled from the top-left edge)",
                          from: CGVector(dx: 0.15, dy: 0.003), to: CGVector(dx: 0.15, dy: 0.7), hold: false)
            await surface(board, settings, slug: "control-centre", name: "Control Centre (pulled from the top-right edge)",
                          from: CGVector(dx: 0.92, dy: 0.003), to: CGVector(dx: 0.92, dy: 0.7), hold: false)
            await surface(board, settings, slug: "app-switcher", name: "the App Switcher (swipe up from the bottom edge and hold)",
                          from: CGVector(dx: 0.5, dy: 0.998), to: CGVector(dx: 0.5, dy: 0.6), hold: true)
        } else {
            fact("system surfaces: could not produce (Settings did not come forward)")
        }

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
        guard let settings = await bringForward(Self.settingsId, icon: "Settings") else { return "Settings did not come forward within 10 s" }
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

    // MARK: - P15: SpringBoard's own states (no capture of an app)

    @MainActor
    func testP15_SpringBoardStates() async throws {
        executionTimeAllowance = 200
        begin("p15-springboard", "SpringBoard showing the Home Screen, the Today view, the App Library, an open folder and jiggle mode: which identifiers in SpringBoard's tree would recognise each, and what does the runner report as front and top layer? (The Home Screen is left clean.)")
        await probeSpringBoardStates()
        await finish(terminating: [])
    }

    @MainActor
    private func probeSpringBoardStates() async {
        await SystemAppAgentTests.startOnHomeScreen(terminating: [])
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        // Run 139: Home lands on the page last shown (page 2 of 2); a second press goes to page 1.
        tolerant("press Home again (to the first page)") { XCUIDevice.shared.press(.home) }
        await pause(1.5)
        note("Home Screen page: \(pageText(board))")
        await springBoardState(board, "home", "the Home Screen (first page)")
        let pages = pageCount(board)

        boardDrag(board, from: 0.15, to: 0.9, "swipe right on the first page (to the Today view)")
        await pause(1.5)
        await springBoardState(board, "today", "the Today view")
        await backHome(board)

        for i in 0..<pages {
            boardDrag(board, from: 0.85, to: 0.1, "swipe left (\(i + 1) of \(pages), past the last page to the App Library)")
            await pause(1.2)
        }
        await springBoardState(board, "app-library", "the App Library")
        await backHome(board)

        let folder = board.icons.matching(NSPredicate(format: "label ENDSWITH %@", "folder")).firstMatch
        if existsNow(folder) {
            var swipes = 0
            while !hittableNow(folder), swipes < pages - 1 {
                boardDrag(board, from: 0.85, to: 0.1, "swipe left to the folder's page")
                swipes += 1
                await pause(1.2)
            }
            if hittableNow(folder) {
                let name = labelNow(folder)
                tolerant("open the folder \(name)") { folder.tap() }
                await pause(1.5)
                await springBoardState(board, "folder", "an open folder (\(name))")
            } else {
                fact("folder: could not produce (the folder icon is not hittable after \(swipes) swipe(s))")
            }
            await backHome(board)
        } else {
            fact("folder: could not produce (no icon labelled \"… folder\" in SpringBoard's tree)")
        }

        // Jiggle mode from a long press on empty wallpaper: on the last page, below its icons
        // (run 139: page 2 holds two rows of icons, ending near y 300 of 956).
        for _ in 0..<max(pages - 1, 0) {
            boardDrag(board, from: 0.85, to: 0.1, "swipe left to the last page")
            await pause(1.2)
        }
        note("page before the long press: \(pageText(board))")
        tolerant("long-press on empty wallpaper (50 %, 62 %)") {
            board.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.62)).press(forDuration: 1.5)
        }
        await pause(1.5)
        await springBoardState(board, "jiggle", "jiggle mode (after a long press on empty wallpaper)")
        if tapFirstButton(board, ["Done"], "leave jiggle mode") == nil {
            tolerant("press Home to leave jiggle mode") { XCUIDevice.shared.press(.home) }
        }
        await pause(1.5)
        await backHome(board)
        let after = capture(board, Self.springboardId, "home-after", "The Home Screen after the probe: no jiggle mode, the icons in place?")
        let doneLeft = existsNow(board.buttons["Done"].firstMatch)
        let settingsIcon = existsNow(board.icons["Settings"].firstMatch)
        let jarvisIcon = existsNow(board.icons["Jarvis"].firstMatch)
        let iconCount = after.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.icon && !$0.label.isEmpty }.count } ?? 0
        fact("Home Screen afterwards: a Done button left \(Self.yesNo(doneLeft)); the Settings icon \(Self.yesNo(settingsIcon)), the Jarvis icon \(Self.yesNo(jarvisIcon)); labelled Icon nodes \(iconCount); page \(pageText(board))")
    }

    /// Captures SpringBoard showing `name`, asks the runner, and records the identifiers that
    /// would recognise the state (every identified non-Icon node, deduplicated, at most 30),
    /// the on-screen icons and buttons, and the runner's front and top layer.
    @MainActor
    private func springBoardState(_ board: XCUIApplication, _ slug: String, _ name: String) async {
        let tree = capture(board, Self.springboardId, slug, "SpringBoard showing \(name): which identifiers would recognise it; what is on screen?")
        let look = await runnerLook("SpringBoard showing \(name)", slug)
        var ids: [String] = []
        var icons = 0
        var buttons: [String] = []
        if let tree {
            for hit in Self.hits(tree, { !$0.identifier.isEmpty && $0.elementType != XCUIElementTypeCode.icon }) {
                let e = hit.element
                let entry = "\(XCUIElementTypeCode.name(e.elementType))#\(Self.clip(e.identifier))" + (Self.onScreen(e.frame, tree.frame) ? "" : " (off screen)")
                if !ids.contains(entry) { ids.append(entry) }
                if ids.count >= 30 { break }
            }
            icons = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.icon && Self.onScreen($0.frame, tree.frame) && !$0.label.isEmpty }.count
            let onScreenButtons = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.button && Self.onScreen($0.frame, tree.frame) && !$0.label.isEmpty }
            buttons = onScreenButtons.prefix(12).map { "\"\(Self.clip($0.element.label))\"" }
        }
        let idText = ids.isEmpty ? "none" : ids.joined(separator: ", ")
        let buttonText = buttons.isEmpty ? "none" : buttons.joined(separator: ", ")
        let typed = look?.root.map { Self.typedCount($0) } ?? 0
        fact("\(name): SpringBoard identifiers: \(idText); labelled Icons on screen \(icons); labelled Buttons on screen: \(buttonText); page \(pageText(board)); the runner's top layer \(lastRunnerFront), its look of \(look?.appBundleId ?? "?") (\(typed) typed nodes)")
    }

    /// Home, twice if needed, until the Home Screen's first page shows (by its page indicator).
    @MainActor
    private func backHome(_ board: XCUIApplication) async {
        tolerant("press Home") { XCUIDevice.shared.press(.home) }
        await pause(1.5)
        if !pageText(board).hasPrefix("Page 1 ") {
            tolerant("press Home again") { XCUIDevice.shared.press(.home) }
            await pause(1.5)
        }
        note("back home: page \(pageText(board))")
    }

    /// A horizontal drag across SpringBoard at mid-height between two normalised widths.
    @MainActor
    private func boardDrag(_ board: XCUIApplication, from: Double, to: Double, _ what: String) {
        tolerant(what) {
            let start = board.coordinate(withNormalizedOffset: CGVector(dx: from, dy: 0.5))
            let end = board.coordinate(withNormalizedOffset: CGVector(dx: to, dy: 0.5))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        note("\(what): dragged from \(Int(from * 100))% to \(Int(to * 100))% of the width")
    }

    /// The Home Screen page indicator's value ("Page 1 of 2"), or "(no page indicator)".
    @MainActor
    private func pageText(_ board: XCUIApplication) -> String {
        var text = "(no page indicator)"
        tolerant("read the page indicator") {
            let indicator = board.pageIndicators.firstMatch
            if indicator.exists { text = (indicator.value as? String) ?? text }
        }
        return text
    }

    /// The number of Home Screen pages from "Page X of N" (2 when it cannot be read).
    @MainActor
    private func pageCount(_ board: XCUIApplication) -> Int {
        let text = pageText(board)
        guard let last = text.split(separator: " ").last, let n = Int(last), n > 0, n < 10 else { return 2 }
        return n
    }
}
