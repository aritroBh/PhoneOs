import Foundation
import Synchronization
import UIKit
import XCTest
import JarvisCore
import JarvisKit

/// Probe round 3, on the Playground's own fixture screens ("Fact probes 3",
/// `ProbeFixtures3.swift`): facts the safety gate assumes but no real iOS 26.5 tree had shown
/// (docs/ARCHITECTURE.md, *What real trees showed*; the audit's P2, P3, P4, P9, P10, P11).
/// Conventions: `Probe3Case`. Each test RECORDS; it fails only when its own set-up cannot run
/// (the app does not launch) and skips, after attaching the evidence, when a state cannot be
/// produced on a simulator.
///
/// Simulator time (typical): P2 ~70 s, P3 ~40 s, P4a ~35 s, P4b ~75 s, P4c ~80 s, P4d ~80 s,
/// P9 ~80 s, P10/P11a/P11b ~55 s each.
final class TreeFactProbe3Tests: Probe3Case {
    /// The symbols of `Probe3SymbolsFixture.symbols`, in the same order.
    static let symbolNames = [
        "trash", "paperplane", "paperplane.fill", "cart", "creditcard",
        "phone", "phone.fill", "square.and.arrow.up", "xmark", "arrow.up.circle.fill",
        "checkmark", "plus", "minus.circle", "bag", "person.crop.circle.badge.plus",
    ]
    /// `Probe3SymbolsFixture.itemNames`: the two toolbars' system items, in order.
    static let toolbarItemNames = ["trash", "action", "compose", "reply", "add", "done", "cancel", "save"]
    static let returnTypeCount = 12

    // MARK: - P4a: symbol-only buttons

    @MainActor
    func testP04a_SymbolButtons() async throws {
        executionTimeAllowance = 120
        begin("p4a-symbols", "What label, identifier and role does each symbol-only button get with no explicit label: SwiftUI Button { Image(systemName:) }, UIKit UIButton with a symbol image, UIToolbar system items?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.symbols") {
            await pause(0.8)
            recordSymbols(app, "symbols", rightToLeft: false)
            await runnerLook("the symbol buttons screen", "symbols")
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Symbol buttons screen not reached?")
            reason = "the Symbol buttons screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    /// Captures the symbols screen and attributes buttons to symbols by the section headers and
    /// reading order (right to left under RTL).
    @MainActor
    private func recordSymbols(_ app: XCUIApplication, _ slug: String, rightToLeft: Bool) {
        let tree = capture(app, Self.appBundleId, slug, "What label, identifier and role does each symbol-only button get (the app sets no accessibilityLabel or identifier)? Where are the bars and what are their roles?")
        guard let tree else { return }
        report(tree, "\(slug): Button nodes", limit: 60) { $0.elementType == XCUIElementTypeCode.button }
        report(tree, "\(slug): Image nodes", limit: 40) { $0.elementType == XCUIElementTypeCode.image }
        report(tree, "\(slug): Toolbar, NavigationBar and TabBar nodes", Self.isType([XCUIElementTypeCode.toolbar, XCUIElementTypeCode.navigationBar, XCUIElementTypeCode.tabBar]))
        let barButtons = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.button }.filter { hit in
            hit.ancestors.contains { $0.elementType == XCUIElementTypeCode.navigationBar }
        }
        let barText = Self.briefs(barButtons)
        fact("\(slug): navigation bar buttons (the first is the Back button): \(barText)")
        if let bar = Self.hits(tree, { $0.elementType == XCUIElementTypeCode.tabBar }).first {
            let items = Self.hits(bar.element) { $0.elementType != XCUIElementTypeCode.tabBar && !$0.label.isEmpty }
            let itemText = Self.briefs(items)
            fact("\(slug): TabBar frame \(Self.rectText(bar.element.frame)); its labelled nodes: \(itemText)")
        } else {
            fact("\(slug): no TabBar node")
        }

        let texts = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.staticText }
        let swiftUIHeader = texts.first(where: { $0.element.label == "SwiftUI symbol buttons" })?.element.frame.y
        let uikitHeader = texts.first(where: { $0.element.label == "UIKit symbol buttons" })?.element.frame.y
        let toolbarHeader = texts.first(where: { $0.element.label == "UIKit toolbar items" })?.element.frame.y
        guard let top = swiftUIHeader, let middle = uikitHeader, let bottom = toolbarHeader else {
            fact("\(slug): symbol attribution: a section header was not found; see the Button lines in the result file")
            return
        }
        let buttons = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.button }.map { $0.element }
        let swiftUI = Self.readingOrder(buttons.filter { Self.midY($0) > top && Self.midY($0) < middle }, rightToLeft)
        let uikit = Self.readingOrder(buttons.filter { Self.midY($0) > middle && Self.midY($0) < bottom }, rightToLeft)
        note("\(slug): buttons between the headers: SwiftUI \(swiftUI.count), UIKit \(uikit.count) (\(Self.symbolNames.count) symbols each)")
        var labelIsName = 0
        var idIsName = 0
        for (i, name) in Self.symbolNames.enumerated() {
            let s = i < swiftUI.count ? Self.brief(swiftUI[i]) : "no button at this place"
            let u = i < uikit.count ? Self.brief(uikit[i]) : "no button at this place"
            if i < swiftUI.count, swiftUI[i].label == name { labelIsName += 1 }
            if i < swiftUI.count, swiftUI[i].identifier == name { idIsName += 1 }
            fact("\(slug): symbol \(name): SwiftUI Button -> \(s); UIKit UIButton -> \(u)")
        }
        fact("\(slug): SwiftUI symbol buttons whose label is the symbol name: \(labelIsName) of \(Self.symbolNames.count); whose identifier is the symbol name: \(idIsName)")
        let toolbars = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.toolbar }.map { $0.element }.filter { Self.midY($0) > bottom }
        var items: [RawElement] = []
        for bar in toolbars.sorted(by: { $0.frame.y < $1.frame.y }) {
            let inBar = Self.hits(bar) { $0.elementType == XCUIElementTypeCode.button || $0.elementType == XCUIElementTypeCode.toolbarButton }.map { $0.element }
            items += Self.readingOrder(inBar, rightToLeft)
        }
        note("\(slug): toolbars below the toolbar header: \(toolbars.count), items in them: \(items.count)")
        for (i, name) in Self.toolbarItemNames.enumerated() {
            let itemText = i < items.count ? Self.brief(items[i]) : "no item at this place"
            fact("\(slug): UIBarButtonItem(systemItem: .\(name)) -> \(itemText)")
        }
    }

    static func midY(_ e: RawElement) -> Double {
        e.frame.y + e.frame.height / 2
    }

    /// Rows top to bottom (8 pt bands), left to right, or right to left under RTL.
    static func readingOrder(_ elements: [RawElement], _ rightToLeft: Bool) -> [RawElement] {
        elements.sorted { a, b in
            let rowA = (midY(a) / 8).rounded()
            let rowB = (midY(b) / 8).rounded()
            if rowA != rowB { return rowA < rowB }
            return rightToLeft ? a.frame.x > b.frame.x : a.frame.x < b.frame.x
        }
    }

    // MARK: - P4b: every UIReturnKeyType

    @MainActor
    func testP04b_ReturnKeys() async throws {
        executionTimeAllowance = 180
        begin("p4b-returnkeys", "For every UIReturnKeyType, what label and identifier does the keyboard's Return key carry, and how is the input host laid out (inputView, Emoji, Dictate against the Keyboard node)?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.returns") {
            reason = await probeReturnKeys(app, count: Self.returnTypeCount, slugPrefix: "return")
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Return keys screen not reached?")
            reason = "the Return keys screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    /// On the Return keys screen: for the first `count` types, focus the field, capture the
    /// keyboard, end editing, step to the next type.
    @MainActor
    private func probeReturnKeys(_ app: XCUIApplication, count: Int, slugPrefix: String) async -> String? {
        let field = app.textFields["probe3.returns.field"].firstMatch
        let next = app.buttons["probe3.returns.next"].firstMatch
        let end = app.buttons["probe3.returns.end"].firstMatch
        let status = app.staticTexts["probe3.returns.status"].firstMatch
        guard waitFor(field, "the return-key field", 5) else {
            capture(app, Self.appBundleId, "\(slugPrefix)-no-field", "Why is the return-key field missing?")
            return "the return-key field was not found"
        }
        var previous = ""
        var distinct = 0
        for i in 0..<count {
            let shown = labelNow(status)
            let name = shown.split(separator: ":").last.map { $0.trimmingCharacters(in: .whitespaces) } ?? "type\(i)"
            tolerant("focus the field") { field.tap() }
            var up = false
            tolerant("wait for the keyboard") { up = app.keyboards.firstMatch.waitForExistence(timeout: 3) }
            if !up { note("no keyboard in Jarvis's tree 3 s after focusing the field (.\(name))") }
            await pause(0.6)
            let tree = capture(app, Self.appBundleId, "\(slugPrefix)-\(name)", "With returnKeyType .\(name): what label and identifier does the Return key carry? Where do inputView, Emoji and Dictate sit against the Keyboard node?")
            let key = reportKeyboard(tree, ".\(name)")
            let text = key.map { Self.brief($0) } ?? "not located"
            fact("UIReturnKeyType.\(name) (status \"\(shown)\"): the Return key (bottom-right key of the Keyboard node) is \(text)")
            if i == 0, let tree { inputHostFacts(tree, "keyboard of a UITextField") }
            if text != previous { distinct += 1 }
            previous = text
            if name == "send" || (count < 3 && i == count - 1) {
                await runnerLook("the keyboard up with returnKeyType .\(name)", "\(slugPrefix)-\(name)")
            }
            tolerant("end editing") { if end.exists { end.tap() } }
            await pause(0.5)
            if i < count - 1 {
                tolerant("step to the next return type") { next.tap() }
                await pause(0.4)
            }
        }
        fact("return keys: the located key changed \(distinct) times over \(count) types (equal to \(count): every type gave a different key)")
        return nil
    }

    // MARK: - P4c: modals

    private struct ModalCase {
        var id: String
        var slug: String
        var name: String
        /// A label that exists only while the modal is up.
        var marker: String
        var destructive: String?
        var dismiss: [String]
        var longPress = false
        var withSpringBoard = false
    }

    private static let modalCases: [ModalCase] = [
        ModalCase(id: "probe3.modals.dialog", slug: "action-sheet", name: "confirmationDialog (action sheet)", marker: "Keep item",
                  destructive: "Delete item", dismiss: ["Cancel"], withSpringBoard: true),
        ModalCase(id: "probe3.modals.alert", slug: "alert-field", name: "alert with a text field", marker: "Probe alert message",
                  destructive: "Delete", dismiss: ["Cancel"], withSpringBoard: true),
        ModalCase(id: "probe3.modals.sheet", slug: "sheet", name: "full sheet", marker: "Probe full sheet body text",
                  destructive: nil, dismiss: ["probe3.modals.close"]),
        ModalCase(id: "probe3.modals.half", slug: "half-sheet", name: "half sheet (.medium detent)", marker: "Probe half sheet body text",
                  destructive: nil, dismiss: ["probe3.modals.close"]),
        ModalCase(id: "probe3.modals.cover", slug: "cover", name: "fullScreenCover", marker: "Probe cover body text",
                  destructive: nil, dismiss: ["probe3.modals.close"]),
        ModalCase(id: "probe3.modals.popover", slug: "popover", name: "popover (compact adaptation .popover)", marker: "Probe popover body text",
                  destructive: nil, dismiss: ["probe3.modals.close"]),
        ModalCase(id: "probe3.modals.menu", slug: "menu", name: "Menu", marker: "Probe menu item",
                  destructive: "Delete from menu", dismiss: []),
        ModalCase(id: "probe3.modals.context", slug: "context-menu", name: "contextMenu", marker: "Probe context item",
                  destructive: "Delete from context menu", dismiss: [], longPress: true),
    ]

    @MainActor
    func testP04c_Modals() async throws {
        executionTimeAllowance = 180
        begin("p4c-modals", "For an action sheet, an alert with a field, a sheet, a half sheet, a full-screen cover, a popover, a Menu and a context menu: is there an Alert/Sheet/dialog-like node, is the content behind still in the tree and is anything marking it covered, what role do destructive buttons have?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.modals") {
            await probeModals(app)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Modals screen not reached?")
            reason = "the Modals screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeModals(_ app: XCUIApplication) async {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let behind = app.staticTexts["probe3.modals.behind"].firstMatch
        _ = waitFor(behind, "the content behind", 5)
        let base = capture(app, Self.appBundleId, "base", "The Modals screen with nothing up: the baseline the modal captures compare with.")
        report(base, "baseline: the content behind", Self.mentions("Probe content behind"))
        note("baseline: the content behind is hittable by query: \(Self.yesNo(hittableNow(behind)))")
        for item in Self.modalCases {
            let trigger = app.descendants(matching: .any)[item.id].firstMatch
            guard waitFor(trigger, "the \(item.name) trigger", 4) else {
                fact("\(item.name): could not produce (the trigger \(item.id) was not found)")
                continue
            }
            if item.longPress {
                tolerant("long-press for the \(item.name)") { trigger.press(forDuration: 1.2) }
            } else {
                tolerant("open the \(item.name)") { trigger.tap() }
            }
            await pause(1.3)
            let marker = Self.node(in: app, labelled: item.marker)
            note("\(item.name): its marker \"\(item.marker)\" found by query: \(Self.yesNo(existsNow(marker)))")
            let tree = capture(app, Self.appBundleId, item.slug, "With the \(item.name) up: is there an Alert/Sheet/dialog-like node, is the content behind still in the tree, is anything marking it covered, what role do destructive buttons have?")
            var boardTree: RawElement?
            if item.withSpringBoard {
                boardTree = capture(board, Self.springboardId, "\(item.slug)-springboard", "With the \(item.name) up: does SpringBoard's tree hold any of it?")
            }
            modalFacts(tree, boardTree, item, behind: behind)
            let look = await runnerLook("the \(item.name) up", item.slug)
            let lookModal = look?.root.map { root in Self.findAll(root) { $0.role == .alert || $0.role == .sheet }.count } ?? 0
            fact("\(item.name): the runner's look is of \(look?.appBundleId ?? "?") with \(lookModal) typed Alert/Sheet node(s)")
            await dismissModal(app, item, marker)
        }
    }

    @MainActor
    private func modalFacts(_ tree: RawElement?, _ boardTree: RawElement?, _ item: ModalCase, behind: XCUIElement) {
        guard let tree else {
            fact("\(item.name): no tree (the capture failed)")
            return
        }
        let modal = report(tree, "\(item.name): modal-like nodes (Alert, Dialog, Sheet, Popover, or an identifier naming a sheet, popover, dimming view or dismiss region)", Self.modalish)
        let behindHits = report(tree, "\(item.name): the content behind (\"Probe content behind\")", Self.mentions("Probe content behind"))
        report(tree, "\(item.name): MenuItem nodes", limit: 12) { $0.elementType == XCUIElementTypeCode.menuItem }
        if item.slug == "alert-field" {
            report(tree, "\(item.name): TextField nodes") { $0.elementType == XCUIElementTypeCode.textField }
        }
        var destructive = "none in this modal"
        if let label = item.destructive {
            let found = report(tree, "\(item.name): nodes labelled \"\(label)\"") { $0.label == label }
            if let hit = found.first {
                let under = Self.yesNo(hit.ancestors.contains(where: Self.modalish))
                destructive = "\(Self.brief(hit.element)), with a modal-like ancestor: \(under)"
            } else {
                destructive = "not in Jarvis's tree"
            }
        }
        let types = Self.typeNames(modal)
        var behindLine = "not in the tree"
        if let hit = behindHits.first {
            let shown = Self.onScreen(hit.element.frame, tree.frame) ? "on screen" : "off screen"
            behindLine = "\(Self.brief(hit.element)) \(shown)"
        }
        let behindHittable = Self.yesNo(hittableNow(behind))
        fact("\(item.name): modal-like nodes \(modal.count) (\(types)); content behind: \(behindLine); hittable by query: \(behindHittable); destructive button: \(destructive)")
        if let boardTree {
            let inBoard = Self.hits(boardTree) { e in
                e.label == item.marker || (item.destructive.map { e.label == $0 } ?? false)
            }
            fact("\(item.name): nodes of it in SpringBoard's tree: \(inBoard.count)")
        }
    }

    /// Closes the modal: its own button, a tap outside, a drag down, and last a relaunch onto
    /// the Modals screen. Each step is checked by the marker being gone.
    @MainActor
    private func dismissModal(_ app: XCUIApplication, _ item: ModalCase, _ marker: XCUIElement) async {
        if !item.dismiss.isEmpty {
            tapFirstButton(app, item.dismiss, "close the \(item.name)")
            await pause(1)
            if !existsNow(marker) { return }
        }
        tapAt(app, 0.5, 0.06, "tap outside the \(item.name)")
        await pause(1)
        if !existsNow(marker) { return }
        drag(app, from: 0.3, to: 0.95, "drag the \(item.name) down")
        await pause(1)
        if !existsNow(marker) { return }
        note("the \(item.name) is still up after every way of closing it: relaunching onto the Modals screen")
        _ = launchPlayground()
        _ = await openProbe3(app, "probe3.modals")
    }

    // MARK: - P4d: controls round 2

    @MainActor
    func testP04d_Controls() async throws {
        executionTimeAllowance = 180
        begin("p4d-controls", "How do a segmented picker, a three-tab TabView, a Link, a NavigationLink, a stepper, compact and wheel date pickers, a toggle, empty and filled secure fields, swipe-action rows (the runner's own swipes) and selected rows appear in the tree?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.controls") {
            await probeControls(app)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Controls 2 screen not reached?")
            reason = "the Controls 2 screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeControls(_ app: XCUIApplication) async {
        let status = app.staticTexts["probe3.controls.status"].firstMatch
        _ = waitFor(status, "the status text", 5)
        let top = capture(app, Self.appBundleId, "controls-top", "Swipe rows, the segmented picker, Link and NavigationLink: roles, labels, values, selected flags; does a link carry its URL?")
        let cells = report(top, "swipe rows: Cell nodes mentioning Swipe row") { e in
            e.elementType == XCUIElementTypeCode.cell && !Self.hits(e, Self.mentions("Swipe row")).isEmpty
        }
        let firstCell = cells.first.map { "; the first: \(Self.brief($0.element)) \(Self.rectText($0.element.frame))" } ?? ""
        fact("swipe rows: Cell nodes holding \"Swipe row\": \(cells.count)\(firstCell)")
        if let top {
            if let segmented = Self.hits(top, { $0.elementType == XCUIElementTypeCode.segmentedControl }).first {
                let segments = Self.hits(segmented.element) { $0.elementType != XCUIElementTypeCode.segmentedControl && !$0.label.isEmpty }
                let segmentText = Self.briefs(segments)
                fact("segmented picker: \(Self.brief(segmented.element)); inside: \(segmentText)")
            } else {
                let parts = Self.hits(top) { ["One", "Two", "Three"].contains($0.label) }
                let partText = Self.briefs(parts)
                fact("segmented picker: no SegmentedControl node; nodes labelled One/Two/Three: \(partText)")
            }
            let links = report(top, "Link nodes") { $0.elementType == XCUIElementTypeCode.link }
            let linkNodes = Self.hits(top, Self.mentions("Probe link"))
            let urlNodes = Self.hits(top, Self.mentions("example.com"))
            let linkText = Self.briefs(linkNodes)
            fact("SwiftUI Link: Link nodes \(links.count); nodes labelled \"Probe link\": \(linkText); its URL (example.com/probe-link) in any label, value or identifier: \(Self.yesNo(!urlNodes.isEmpty))")
            let navigation = Self.hits(top, Self.mentions("Probe navigation link"))
            let navigationText = Self.briefs(navigation, empty: "not found")
            fact("NavigationLink: \(navigationText)")
        }

        await probeRowSwipes(app, status)
        await probeTabs(app)

        drag(app, from: 0.8, to: 0.35, "scroll to the values")
        await pause(1.2)
        let values = capture(app, Self.appBundleId, "controls-values", "Stepper, compact and wheel date pickers, toggle, empty and filled secure fields: roles, labels, values; what value does a filled secure field report?")
        let secure = report(values, "SecureTextField nodes") { $0.elementType == XCUIElementTypeCode.secureTextField }
        let secureParts: [String] = secure.map { hit in "\(Self.brief(hit.element)) placeholder \"\(hit.element.placeholder ?? "")\"" }
        let secureText = secureParts.isEmpty ? "no SecureTextField node" : secureParts.joined(separator: "; ")
        fact("secure fields: \(secureText)")
        let steppers = report(values, "Stepper, IncrementArrow, DecrementArrow and buttons labelled Increment/Decrement", limit: 12) { e in
            [XCUIElementTypeCode.stepper, XCUIElementTypeCode.incrementArrow, XCUIElementTypeCode.decrementArrow].contains(e.elementType)
                || e.label.contains("Increment") || e.label.contains("Decrement")
        }
        let stepperText = Self.briefs(steppers, empty: "no stepper-like node")
        fact("stepper: \(stepperText)")
        let pickers = report(values, "DatePicker, Picker and PickerWheel nodes", limit: 12, Self.isType([XCUIElementTypeCode.datePicker, XCUIElementTypeCode.picker, XCUIElementTypeCode.pickerWheel]))
        let pickerText = Self.briefs(pickers)
        fact("date pickers: \(pickerText)")
        let switches = report(values, "Switch and Toggle nodes", Self.isType([XCUIElementTypeCode.`switch`, XCUIElementTypeCode.toggle]))
        let switchText = Self.briefs(switches)
        fact("toggle: \(switchText)")
        await probeCompactDate(app)

        drag(app, from: 0.8, to: 0.3, "scroll to the selection rows")
        await pause(1.2)
        let selection = capture(app, Self.appBundleId, "controls-selection", "An inline picker (checkmark rows) and rows marked selected with a checkmark and the isSelected trait: how does a selected row appear?")
        let inline = report(selection, "inline picker rows (Alpha, Beta, Gamma)", limit: 12) { e in
            ["Alpha", "Beta", "Gamma"].contains(e.label) || ["Alpha", "Beta", "Gamma"].contains(where: { e.label.hasPrefix($0 + ",") })
        }
        let inlineText = Self.briefs(inline, empty: "rows not found")
        fact("inline picker (Beta chosen): \(inlineText)")
        let checks = report(selection, "check rows", limit: 12, Self.mentions("Check row"))
        let checkText = Self.briefs(checks, empty: "rows not found")
        fact("checkmark rows (row 2 chosen, isSelected trait on it): \(checkText)")
        report(selection, "checkmark images", Self.mentions("checkmark"))
    }

    /// The runner's own swipes on a swipe-action row: the core's path for "swipe left" on
    /// row 1, then a full-width swipe (safe area edge to edge) on row 2.
    @MainActor
    private func probeRowSwipes(_ app: XCUIApplication, _ status: XCUIElement) async {
        let runner = XCUITestPhoneRunner()
        let look = await runnerLook("Controls 2, before the row swipes", "controls-before-swipes", runner: runner)
        guard let root = look?.root, let screen = look?.screenSize else {
            fact("row swipes: could not produce (no runner look)")
            return
        }
        if let frame = Self.rowFrame(root, "Swipe row 1", screen), let path = Self.coreSwipe("left", frame: frame, screen: screen) {
            await runnerDo(runner, .swipe(from: path.from, to: path.to, durationMs: 200), expecting: Self.appBundleId, "swipe left on Swipe row 1 (the core's path for a swipe left on the row)")
            await pause(1.5)
            let after = capture(app, Self.appBundleId, "row1-after-runner-swipe", "After the runner's swipe left on a row whose trailing actions allow a full swipe: is the row still there, which actions were revealed (role, label, identifier)?")
            let still = after.map { !Self.hits($0, Self.mentions("Swipe row 1")).isEmpty } ?? false
            let revealed = report(after, "revealed actions (buttons labelled Delete or Flag)") { e in e.label == "Delete" || e.label == "Flag" }
            let revealedText = Self.briefs(revealed)
            let statusText = labelNow(status)
            fact("runner swipe left on a row (core path \(Self.pointText(path.from)) -> \(Self.pointText(path.to)), row \(Self.rectText(frame))): row still in the tree \(Self.yesNo(still)); status \"\(statusText)\"; revealed: \(revealedText)")
            if !revealed.isEmpty, let back = Self.coreSwipe("right", frame: frame, screen: screen) {
                await runnerDo(runner, .swipe(from: back.from, to: back.to, durationMs: 200), expecting: Self.appBundleId, "swipe right on row 1 to close its actions")
                await pause(1)
            }
        } else {
            fact("row swipes: Swipe row 1 not found in the runner's look")
        }
        let second = await runnerLook("Controls 2, before the full-width swipe", "controls-before-full-swipe", runner: runner)
        guard let root2 = second?.root, let frame2 = Self.rowFrame(root2, "Swipe row 2", screen) else {
            fact("full-width row swipe: Swipe row 2 not found in the runner's look")
            return
        }
        let y = frame2.y + frame2.height / 2
        let from = Point(x: screen.width * 0.96, y: y)
        let to = Point(x: screen.width * 0.04, y: y)
        await runnerDo(runner, .swipe(from: from, to: to, durationMs: 200), expecting: Self.appBundleId, "full-width swipe left on Swipe row 2 (safe-area edge to edge)")
        await pause(1.5)
        let after = capture(app, Self.appBundleId, "row2-after-full-swipe", "After the runner's full-width swipe left on a row with a full-swipe destructive action: was the row deleted?")
        let still = after.map { !Self.hits($0, Self.mentions("Swipe row 2")).isEmpty } ?? false
        fact("runner full-width swipe left (\(Self.pointText(from)) -> \(Self.pointText(to))): row 2 still in the tree \(Self.yesNo(still)); status \"\(labelNow(status))\"")
        if still {
            let revealed = after.map { tree in Self.hits(tree) { $0.label == "Delete" } } ?? []
            if !revealed.isEmpty, let back = Self.coreSwipe("right", frame: frame2, screen: screen) {
                await runnerDo(runner, .swipe(from: back.from, to: back.to, durationMs: 200), expecting: Self.appBundleId, "swipe right on row 2 to close its actions")
                await pause(1)
            }
        }
    }

    /// The three-tab TabView (in a full-screen cover): the tab items' role, selected flag and
    /// container, before and after choosing tab 2.
    @MainActor
    private func probeTabs(_ app: XCUIApplication) async {
        let open = app.buttons["probe3.controls.tabs"].firstMatch
        guard waitFor(open, "the Show tabs button", 4) else {
            fact("TabView: could not produce (no Show tabs button)")
            return
        }
        tolerant("open the tabs cover") { open.tap() }
        await pause(1.5)
        let first = capture(app, Self.appBundleId, "tabs", "A TabView with three tabs: what are the tab items (role, selected flag) and their container's role?")
        tabFacts(first, "tab 1 chosen")
        await runnerLook("the TabView cover, tab 1", "tabs")
        tolerant("choose Probe tab 2") {
            let tab = app.buttons["Probe tab 2"].firstMatch
            if tab.exists { tab.tap() } else { note("no button labelled Probe tab 2") }
        }
        await pause(1)
        let second = capture(app, Self.appBundleId, "tabs-second", "The same TabView after choosing tab 2: which item carries the selected flag?")
        tabFacts(second, "tab 2 chosen")
        var closed = false
        tolerant("close the tabs cover") {
            for button in app.buttons.matching(identifier: "probe3.tabs.close").allElementsBoundByIndex where !closed {
                if button.isHittable {
                    button.tap()
                    closed = true
                }
            }
        }
        await pause(1)
        if !closed || existsNow(Self.node(in: app, containing: "Probe tab content")) {
            note("the tabs cover did not close by its button: relaunching onto Controls 2")
            _ = launchPlayground()
            _ = await openProbe3(app, "probe3.controls")
        }
    }

    @MainActor
    private func tabFacts(_ tree: RawElement?, _ what: String) {
        guard let tree else {
            fact("TabView (\(what)): no tree")
            return
        }
        let bars = report(tree, "TabView (\(what)): TabBar, TabGroup and Tab nodes", Self.isType([XCUIElementTypeCode.tabBar, XCUIElementTypeCode.tabGroup, XCUIElementTypeCode.tab]))
        let items = Self.hits(tree, Self.mentions("Probe tab ")).filter { !$0.element.label.hasPrefix("Probe tab content") }
        let parentNames: [String] = items.compactMap { hit in hit.ancestors.last.map { XCUIElementTypeCode.name($0.elementType) } }
        let containerList = Set(parentNames).sorted().joined(separator: ", ")
        let containers = containerList.isEmpty ? "-" : containerList
        let barText = Self.typeNames(bars)
        let itemText = Self.briefs(items)
        fact("TabView (\(what)): bar-like nodes \(bars.count) (\(barText)); tab items: \(itemText); their parents' types: \(containers)")
    }

    /// The compact date picker opened: what container holds the calendar?
    @MainActor
    private func probeCompactDate(_ app: XCUIApplication) async {
        let picker = app.descendants(matching: .any)["probe3.controls.compactdate"].firstMatch
        guard existsNow(picker) else {
            fact("compact date picker: could not produce (no element with id probe3.controls.compactdate)")
            return
        }
        tolerant("open the compact date picker") { picker.coordinate(withNormalizedOffset: CGVector(dx: 0.85, dy: 0.5)).tap() }
        await pause(1.5)
        let open = capture(app, Self.appBundleId, "compact-date-open", "A compact DatePicker opened: what container holds the calendar (Popover, Sheet, Other), and is the form behind still in the tree?")
        let modal = report(open, "compact date open: modal-like nodes", Self.modalish)
        let behind = open.map { !Self.hits($0, Self.mentions("Probe toggle")).isEmpty } ?? false
        let modalText = Self.typeNames(modal)
        fact("compact date picker open: modal-like nodes \(modal.count) (\(modalText)); the form behind (\"Probe toggle\") still in the tree: \(Self.yesNo(behind))")
        await runnerLook("the compact date picker open", "compact-date-open")
        tapAt(app, 0.5, 0.06, "tap outside the date popover")
        await pause(1)
    }

    // MARK: - P2: out-of-process pickers

    private struct PickerCase {
        var id: String
        var slug: String
        var name: String
        var markers: [String]
    }

    private static let pickerCases: [PickerCase] = [
        PickerCase(id: "probe3.pickers.photos", slug: "photo-picker", name: "PHPickerViewController",
                   markers: ["Photos", "Albums", "Collections", "Cancel", "Close"]),
        PickerCase(id: "probe3.pickers.contacts", slug: "contact-picker", name: "CNContactPickerViewController",
                   markers: ["Contacts", "John Appleseed", "Kate Bell", "Cancel"]),
        PickerCase(id: "probe3.pickers.documents", slug: "document-picker", name: "UIDocumentPickerViewController",
                   markers: ["Browse", "Recents", "Cancel", "Open"]),
        PickerCase(id: "probe3.pickers.message", slug: "message-composer", name: "MFMessageComposeViewController",
                   markers: ["To:", "Send", "Cancel", "Probe message body"]),
    ]

    @MainActor
    func testP02_OutOfProcessPickers() async throws {
        executionTimeAllowance = 180
        begin("p2-pickers", "For the photo, contact and document pickers and the message composer over the Playground: is the host's content still in the host's snapshot, is the sheet's content there, are frames screen coordinates, which process owns it, what does the runner report?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.pickers") {
            await probePickers(app)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Pickers screen not reached?")
            reason = "the Pickers screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probePickers(_ app: XCUIApplication) async {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let behind = app.staticTexts["probe3.pickers.behind"].firstMatch
        let status = app.staticTexts["probe3.pickers.status"].firstMatch
        for item in Self.pickerCases {
            let button = app.buttons[item.id].firstMatch
            guard waitFor(button, "the \(item.name) button", 5), hittableNow(button) else {
                fact("\(item.name): could not produce (its button is not hittable: an earlier sheet may still be up)")
                continue
            }
            tolerant("open the \(item.name)") { button.tap() }
            await pause(3)
            let shown = labelNow(status)
            if shown.contains("canSendText false") {
                fact("\(item.name): could not produce on this simulator: MFMessageComposeViewController.canSendText() is false")
                continue
            }
            let tree = capture(app, Self.appBundleId, item.slug, "With the \(item.name) up over the Playground: is the host's content still in the host's snapshot, is the sheet's content there, and are its frames screen coordinates?")
            let boardTree = capture(board, Self.springboardId, "\(item.slug)-springboard", "With the \(item.name) up: does SpringBoard's tree hold it?")
            attachQuery(app, item.slug, "With the \(item.name) up: what does XCUITest's query view reach (remote content the snapshot may not hold)?")
            let inApp = report(tree, "\(item.name): its content in Jarvis's snapshot (nodes labelled like it)") { item.markers.contains($0.label) }
            let inBoard = report(boardTree, "\(item.name): its content in SpringBoard's snapshot") { item.markers.contains($0.label) }
            report(tree, "\(item.name): modal-like and remote-container nodes in Jarvis's snapshot") { e in
                Self.modalish(e) || e.identifier.localizedCaseInsensitiveContains("remote")
            }
            let hostInTree = tree.map { !Self.hits($0, Self.mentions("Probe host content")).isEmpty } ?? false
            var queried: [String] = []
            for marker in item.markers {
                if let frame = queryFrame(Self.node(in: app, labelled: marker)) { queried.append("\"\(marker)\" \(Self.frameText(frame))") }
            }
            let queriedText = queried.isEmpty ? "nothing" : queried.joined(separator: ", ")
            let behindHittable = Self.yesNo(hittableNow(behind))
            fact("\(item.name): host content in Jarvis's snapshot \(Self.yesNo(hostInTree)), hittable by query \(behindHittable); its content in Jarvis's snapshot \(inApp.count) node(s), in SpringBoard's \(inBoard.count); reached by query: \(queriedText)")
            if let hit = inApp.first, let frame = queryFrame(Self.node(in: app, labelled: hit.element.label)) {
                fact("\(item.name): \"\(hit.element.label)\" snapshot frame \(Self.rectText(hit.element.frame)) vs query frame \(Self.frameText(frame)) (equal: the snapshot holds screen coordinates)")
            }
            let look = await runnerLook("the \(item.name) up", item.slug)
            let lookHolds = look?.root.map { root in Self.findAll(root) { item.markers.contains($0.label ?? "") }.count } ?? 0
            fact("\(item.name): the runner's look is of \(look?.appBundleId ?? "?"), holding \(lookHolds) node(s) labelled like the picker")
            await dismissPicker(app, item, button)
        }
    }

    @MainActor
    private func dismissPicker(_ app: XCUIApplication, _ item: PickerCase, _ button: XCUIElement) async {
        tapFirstButton(app, ["Cancel", "Close", "Done"], "close the \(item.name)")
        await pause(1.5)
        if hittableNow(button) { return }
        drag(app, from: 0.2, to: 0.95, "drag the \(item.name) down")
        await pause(1.5)
        if hittableNow(button) { return }
        note("the \(item.name) is still up: relaunching onto the Pickers screen")
        _ = launchPlayground()
        _ = await openProbe3(app, "probe3.pickers")
    }

    // MARK: - P9: permission prompts

    private static let permissionCases: [(id: String, slug: String, name: String)] = [
        ("probe3.perm.location", "location", "location when in use"),
        ("probe3.perm.camera", "camera", "camera"),
        ("probe3.perm.microphone", "microphone", "microphone"),
        ("probe3.perm.contacts", "contacts", "contacts"),
        ("probe3.perm.photos", "photos", "photo library"),
        ("probe3.perm.tracking", "tracking", "tracking transparency"),
        ("probe3.perm.localnet", "localnet", "local network"),
        ("probe3.perm.paste", "paste", "paste (pasteboard written by the test runner)"),
    ]

    @MainActor
    func testP09_PermissionPrompts() async throws {
        executionTimeAllowance = 240
        begin("p9-permissions", "For each permission prompt raised from the Playground: is it an Alert in SpringBoard's tree only, what are its buttons' labels and identifiers, does the runner's top-layer reading see it? (Each is denied.)")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.permissions") {
            await probePermissions(app)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Permissions screen not reached?")
            reason = "the Permissions screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probePermissions(_ app: XCUIApplication) async {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let status = app.staticTexts["probe3.perm.status"].firstMatch
        for item in Self.permissionCases {
            if item.slug == "paste" {
                UIPasteboard.general.string = "Probe paste 5521"
                note("the test runner (another process) wrote \"Probe paste 5521\" to the general pasteboard")
            }
            let button = app.buttons[item.id].firstMatch
            guard waitFor(button, "the \(item.name) button", 5) else {
                fact("\(item.name): could not produce (no button)")
                continue
            }
            tolerant("ask for \(item.name)") { button.tap() }
            let owner = await waitForAlert([(Self.springboardId, board), (Self.appBundleId, app)], 6)
            await pause(0.5)
            let boardTree = capture(board, Self.springboardId, "\(item.slug)-springboard", "After asking for \(item.name): is the prompt an Alert in SpringBoard's tree? Its buttons' labels and identifiers?")
            let appTree = capture(app, Self.appBundleId, "\(item.slug)-jarvis", "After asking for \(item.name): does Jarvis's own tree hold the prompt?")
            let inBoard = boardTree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.alert }.count } ?? 0
            let inApp = appTree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.alert }.count } ?? 0
            let buttons = (boardTree.map { Self.alertButtons($0) } ?? []) + (appTree.map { Self.alertButtons($0) } ?? [])
            guard let (ownerId, ownerApp) = owner else {
                fact("\(item.name): no Alert in SpringBoard's or Jarvis's tree within 6 s (Alert nodes: SpringBoard \(inBoard), Jarvis \(inApp)); status \"\(labelNow(status))\"")
                await pause(0.5)
                continue
            }
            let labels = readAlert(ownerApp, ownerId, item.name)
            let look = await runnerLook("the \(item.name) prompt up", item.slug)
            let deny = Self.denyingLabel(labels) ?? labels.last
            let quoted: [String] = labels.map { "\"\($0)\"" }
            let buttonText = buttons.isEmpty ? quoted.joined(separator: ", ") : buttons.joined(separator: "; ")
            let lookApp = look?.appBundleId ?? "?"
            let denyText = deny ?? "nothing to tap"
            fact("\(item.name): the prompt is an Alert in \(ownerId)'s tree by query; Alert nodes in the snapshots: SpringBoard \(inBoard), Jarvis \(inApp); buttons: \(buttonText); the runner's top layer: \(lookApp); denied with \"\(denyText)\"")
            if let deny { await tapInAlert(ownerApp, deny, "deny the \(item.name) prompt") }
            await pause(0.8)
            note("\(item.name): status afterwards \"\(labelNow(status))\"")
        }
    }

    // MARK: - P3: an incoming CallKit call

    @MainActor
    func testP03_IncomingCall() async throws {
        executionTimeAllowance = 120
        begin("p3-call", "When an app reports an incoming CallKit call, which process's tree holds the call UI (it is neither a banner nor a SpringBoard alert by assumption), and what does the runner report as front and top layer?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.system") {
            reason = await probeCall(app)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the System screen not reached?")
            reason = "the System screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    @MainActor
    private func probeCall(_ app: XCUIApplication) async -> String? {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let inCallId = "com.apple.InCallService"
        let inCall = XCUIApplication(bundleIdentifier: inCallId)
        let status = app.staticTexts["probe3.system.status"].firstMatch
        let call = app.buttons["probe3.system.call"].firstMatch
        guard waitFor(call, "the Report incoming call button", 5) else { return "no Report incoming call button" }
        tolerant("report an incoming call") { call.tap() }
        var shown = ""
        let deadline = Date().addingTimeInterval(8)
        while Date() < deadline {
            shown = labelNow(status)
            if shown.hasPrefix("Call report failed") || shown == "Call reported" { break }
            await pause(0.4)
        }
        await pause(1.5)
        shown = labelNow(status)
        note("status \"\(shown)\"; InCallService state \(stateText(inCall)); SpringBoard state \(stateText(board))")
        if !shown.hasPrefix("Call reported") && !shown.hasPrefix("Call answered") {
            capture(app, Self.appBundleId, "call-not-reported", "Why did CallKit not report the call?")
            fact("incoming call: could not produce on this simulator: the app's status is \"\(shown)\"")
            return "CallKit did not report the incoming call on this simulator: \(shown)"
        }
        let appTree = capture(app, Self.appBundleId, "call-jarvis", "With an incoming call reported: does Jarvis's tree hold the call UI?")
        let boardTree = capture(board, Self.springboardId, "call-springboard", "With an incoming call reported: does SpringBoard's tree hold the call UI (banner, alert, other)?")
        var callTree: RawElement?
        if isRunning(inCall) {
            callTree = capture(inCall, inCallId, "call-incallservice", "With an incoming call reported: does InCallService's tree hold the call UI?")
        } else {
            note("InCallService is not running (state \(stateText(inCall))): its tree is not captured")
        }
        let caller = Self.mentions("Probe caller")
        let inApp = report(appTree, "Jarvis's tree: nodes mentioning Probe caller", caller)
        let inBoard = report(boardTree, "SpringBoard's tree: nodes mentioning Probe caller", caller)
        let inService = report(callTree, "InCallService's tree: nodes mentioning Probe caller", caller)
        report(boardTree, "SpringBoard's tree: Accept and Decline buttons and modal-like nodes") { e in
            e.label == "Accept" || e.label == "Decline" || Self.modalish(e)
        }
        report(callTree, "InCallService's tree: Accept and Decline buttons") { e in e.label == "Accept" || e.label == "Decline" }
        let look = await runnerLook("an incoming call up", "call")
        fact("incoming call: nodes naming the caller: Jarvis \(inApp.count), SpringBoard \(inBoard.count), InCallService \(isRunning(inCall) ? String(inService.count) : "not running"); the runner's look is of \(look?.appBundleId ?? "?")")
        var declined = ""
        for (id, owner) in [(Self.springboardId, board), (inCallId, inCall)] where declined.isEmpty {
            let decline = owner.buttons["Decline"].firstMatch
            if existsNow(decline) {
                tolerant("tap Decline in \(id)") { decline.tap() }
                declined = id
            }
        }
        await pause(2)
        note(declined.isEmpty ? "no Decline button found by query in SpringBoard or InCallService" : "tapped Decline in \(declined)")
        tolerant("end the call from the app") {
            let end = app.buttons["probe3.system.endcall"].firstMatch
            if end.exists && end.isHittable { end.tap() }
        }
        await pause(1)
        fact("incoming call: declined via \(declined.isEmpty ? "nothing (the app ended it)" : declined); status afterwards \"\(labelNow(status))\"")
        return nil
    }

    // MARK: - P10 / P11: language, text size, writing direction

    @MainActor
    func testP10_German() async throws {
        try await environmentProbe("p10-german", ["-AppleLanguages", "(de)", "-AppleLocale", "de_DE"], rightToLeft: false,
                                   "Under German (launch arguments): which system-provided labels change (bar Back button, symbol descriptions, the Return key's label against its identifier), and do bar frames and roles change?")
    }

    @MainActor
    func testP11a_AccessibilityXXXL() async throws {
        try await environmentProbe("p11a-xxxl", ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"], rightToLeft: false,
                                   "At accessibility XXXL Dynamic Type (launch argument): do bar and tab-bar frames and roles change, and do the symbol buttons and the keyboard keep their labels?")
    }

    @MainActor
    func testP11b_RightToLeft() async throws {
        try await environmentProbe("p11b-rtl", ["-AppleTextDirection", "YES", "-NSForceRightToLeftWritingDirection", "YES"], rightToLeft: true,
                                   "Right to left (launch arguments): are frames mirrored, do labels or roles change, where do the Back button and the keyboard's Return key sit?")
    }

    @MainActor
    private func environmentProbe(_ name: String, _ arguments: [String], rightToLeft: Bool, _ question: String) async throws {
        executionTimeAllowance = 150
        begin(name, question)
        note("launch arguments: \(arguments.joined(separator: " "))")
        let app = launchPlayground(extraArguments: arguments)
        var reason: String?
        if await openProbe3(app, "probe3.symbols") {
            await pause(0.8)
            recordSymbols(app, "symbols", rightToLeft: rightToLeft)
            let field = app.textFields["probe3.symbols.field"].firstMatch
            tolerant("focus the symbols field") { field.tap() }
            await pause(1.2)
            let tree = capture(app, Self.appBundleId, "symbols-keyboard", "Under these settings: the keyboard of a default SwiftUI text field: the Return key's label against its identifier, Emoji and Dictate labels")
            let key = reportKeyboard(tree, "symbols field")
            let keyText = key.map { Self.brief($0) } ?? "not located"
            fact("default SwiftUI TextField: the Return key is \(keyText)")
            if let tree { inputHostFacts(tree, "keyboard of a SwiftUI TextField") }
            await runnerLook("the symbols screen with the keyboard up", "symbols-keyboard")
            let back = app.navigationBars.buttons.element(boundBy: 0)
            tolerant("go back to Fact probes 3") { if back.exists { back.tap() } }
            await pause(1)
            let row = app.buttons["probe3.returns"].firstMatch
            if waitFor(row, "the Return keys row", 5) {
                tolerant("open Return keys") { row.tap() }
                await pause(1)
                _ = await probeReturnKeys(app, count: 2, slugPrefix: "return")
            }
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Symbol buttons screen not reached under these settings?")
            reason = "the Symbol buttons screen was not reached under \(name)"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }
}

// MARK: - Shared machinery (also used by TreeFactProbe3SystemTests)

/// Shared machinery for probe round 3 (`TreeFactProbe3Tests`, `TreeFactProbe3SystemTests`):
/// the round-1/2 probes' recording conventions (`TreeFactProbeTests`), with every attachment
/// named `probe3-<probe>-<nn>-<what>` and opened by the question it answers.
///
/// A probe RECORDS. Per state it attaches (a) the raw tree of each process it can name
/// (`capture`: the harness dump, typed tree then raw snapshot, plus a screenshot) and (b) what
/// Jarvis's runner reports (`runnerLook`: `XCUITestPhoneRunner.frontBundleId()` and its own
/// snapshot, attached as `…-runner-tree.txt`); the difference is the finding. It ends with
/// `probe3-<probe>-result.txt` (every step, in time order) and `probe3-<probe>-facts.txt` (the
/// questions answered mechanically). A surprising iOS behaviour is recorded, never a failure:
/// XCTest issues raised while probing are captured as data (`tolerant`), every wait is bounded,
/// and a state that cannot be produced on a simulator ends the probe with `XCTSkip` after the
/// evidence is attached. Each probe leaves the simulator on the Home Screen, in portrait.
class Probe3Case: QATestCase {
    static let springboardId = "com.apple.springboard"
    static let settingsId = "com.apple.Preferences"
    static let safariId = "com.apple.mobilesafari"

    var probe = "probe3"
    var captures = 0
    var lines: [String] = []
    var facts: [String] = []
    var started = Date()
    /// The last observation a `runnerLook` runner reported (main actor only).
    var lastRunnerRecord: RunnerObservationRecord?
    private var pendingRunnerSlug = "runner"
    private var pendingRunnerQuestion = ""

    override func tearDownWithError() throws {
        // Never leave the simulator rotated, even when a probe ended early (teardown runs on
        // the main thread; checked, not assumed).
        if Thread.isMainThread {
            MainActor.assumeIsolated {
                if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
            }
        }
        try super.tearDownWithError()
    }

    // MARK: - Recording

    @MainActor
    func begin(_ name: String, _ question: String) {
        continueAfterFailure = true
        probe = name
        captures = 0
        lines = []
        facts = []
        started = Date()
        note("probe round 3, \(name), started (\(ProcessInfo.processInfo.operatingSystemVersionString))")
        note("QUESTION: \(question)")
    }

    @MainActor
    func note(_ text: String) {
        lines.append(String(format: "t+%6.1f s  ", Date().timeIntervalSince(started)) + text)
    }

    /// One mechanically answered fact (`question: answer`), for the facts attachment.
    @MainActor
    func fact(_ text: String) {
        facts.append(text)
        note("FACT \(text)")
    }

    /// Leaves the simulator as the other tests expect it (portrait, Home Screen, Jarvis and the
    /// named apps terminated) and writes the result and facts attachments.
    @MainActor
    func finish(terminating ids: [String]) async {
        if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
        let own = XCUIApplication()
        if own.state != .notRunning {
            tolerant("terminate Jarvis") { own.terminate() }
        }
        await SystemAppAgentTests.startOnHomeScreen(terminating: ids)
        note("probe ended: Home Screen, portrait")
        let text = "PROBE ROUND 3 \(probe): every step in time order\n" + lines.joined(separator: "\n") + "\n"
        attach(XCTAttachment(string: text), "probe3-\(probe)-result.txt")
        let body = facts.isEmpty ? "(no fact could be read mechanically: see the result file)\n" : facts.map { "- " + $0 }.joined(separator: "\n") + "\n"
        let factText = "PROBE ROUND 3 \(probe): the questions answered mechanically (question: answer)\n" + body
        attach(XCTAttachment(string: factText), "probe3-\(probe)-facts.txt")
        print(text)
        print(factText)
    }

    /// Captures `app`'s tree and a screenshot as `probe3-<probe>-NN-<slug>-tree.txt` and
    /// `….png`, opened by `question`. Returns the raw copy, or nil (recorded) when the snapshot
    /// failed.
    @MainActor
    @discardableResult
    func capture(_ app: XCUIApplication, _ bundleId: String, _ slug: String, _ question: String) -> RawElement? {
        captures += 1
        let n = String(format: "%02d", captures)
        let base = "probe3-\(probe)-\(n)-\(slug)"
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
            note("capture \(n) (\(slug)): \(bundleId) snapshot failed (state \(state.rawValue)): \(failure)")
            attach(XCTAttachment(string: "Q: \(question)\ncapture \(n) (\(slug)): \(bundleId), state \(state.rawValue): the snapshot failed: \(failure)\n"), "\(base)-tree.txt")
            return nil
        }
        let ms = Int(Date().timeIntervalSince(start) * 1_000)
        let frame = raw.frame.sanitized
        let size: Size? = frame.width > 0 && frame.height > 0 ? Size(width: frame.width, height: frame.height) : nil
        let built = UiTreeBuilder.build(raw, screenSize: size)
        let shot = XCUIScreen.main.screenshot()
        let record = RunnerObservationRecord(
            number: captures, bundleId: bundleId, raw: raw, built: built, screenshot: shot,
            notes: ["probe round 3 \(probe), capture \(n): \(slug)",
                    "app state \(state.rawValue), device orientation \(XCUIDevice.shared.orientation.rawValue)"],
            snapshotMs: ms
        )
        let header = "Q: \(question)\ncapture \(n) (\(slug)): the raw tree of \(bundleId) by XCUITest's snapshot (this probe's own capture, not the runner's choice of process)\n\n"
        attach(XCTAttachment(string: header + QARun.treeText(record)), "\(base)-tree.txt")
        attach(XCTAttachment(screenshot: shot), "\(base).png")
        note("capture \(n) (\(slug)): \(bundleId), state \(state.rawValue), \(built.nodeCount) nodes, snapshot \(ms) ms -> \(base)-tree.txt")
        return raw
    }

    /// Attaches `app.debugDescription` (XCUITest's query view, which reaches remote content the
    /// snapshot may not) as `probe3-<probe>-NN-<slug>-query.txt`.
    @MainActor
    func attachQuery(_ app: XCUIApplication, _ slug: String, _ question: String) {
        captures += 1
        let n = String(format: "%02d", captures)
        let start = Date()
        var text = "(not read)"
        tolerant("read debugDescription") { text = app.debugDescription }
        let ms = Int(Date().timeIntervalSince(start) * 1_000)
        let name = "probe3-\(probe)-\(n)-\(slug)-query.txt"
        attach(XCTAttachment(string: "Q: \(question)\ncapture \(n) (\(slug)): XCUIApplication.debugDescription (the query view), read in \(ms) ms\n\n" + text), name)
        note("capture \(n) (\(slug)): debugDescription, \(text.count) characters, \(ms) ms -> \(name)")
    }

    @MainActor
    func attach(_ attachment: XCTAttachment, _ name: String) {
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    // MARK: - What the runner reports

    /// What Jarvis's own runner reports now (the code the agent runs): `frontBundleId()` (its
    /// top layer) and `snapshot(includeScreenshot: false)` (front id, typed tree, notes); the
    /// observation is attached as `probe3-<probe>-NN-<slug>-runner-tree.txt`. A fresh runner,
    /// with no app noted as activated, unless `given`.
    @MainActor
    @discardableResult
    func runnerLook(_ moment: String, _ slug: String, runner given: XCUITestPhoneRunner? = nil) async -> RunnerObservation? {
        let runner = given ?? XCUITestPhoneRunner()
        pendingRunnerSlug = slug
        pendingRunnerQuestion = "What does Jarvis's runner (XCUITestPhoneRunner, the code the agent runs) report: \(moment)? Compare with the raw captures next to it."
        lastRunnerRecord = nil
        runner.onObservation = { [weak self] record in self?.keepRunnerRecord(record) }
        var front = "?"
        do {
            let id = try await runner.frontBundleId()
            front = id ?? "nil"
        } catch {
            front = "threw \(error)"
        }
        note("\(moment): the runner's frontBundleId() (its top layer) is \(front)")
        var observation: RunnerObservation?
        do {
            let seen = try await runner.snapshot(includeScreenshot: false)
            observation = seen
            let count = seen.root.map { Self.typedCount($0) } ?? 0
            let size = "\(seen.screenSize.width.compactText)x\(seen.screenSize.height.compactText)"
            let keyboard = seen.keyboardVisible ? "visible" : "hidden"
            let notes = seen.notes.isEmpty ? "none" : seen.notes.joined(separator: " | ")
            note("\(moment): the runner's look is of \(seen.appBundleId), \(count) typed nodes, keyboard \(keyboard), orientation \(seen.orientation), screen \(size); notes: \(notes)")
        } catch {
            note("\(moment): the runner's snapshot threw \(error)")
        }
        runner.onObservation = nil
        return observation
    }

    @MainActor
    private func keepRunnerRecord(_ record: RunnerObservationRecord) {
        lastRunnerRecord = record
        captures += 1
        let n = String(format: "%02d", captures)
        let name = "probe3-\(probe)-\(n)-\(pendingRunnerSlug)-runner-tree.txt"
        let header = "Q: \(pendingRunnerQuestion)\ncapture \(n): the runner's own observation \(record.number), of \(record.bundleId) (the process it chose as the top layer), \(record.built.nodeCount) nodes\n\n"
        attach(XCTAttachment(string: header + QARun.treeText(record)), name)
        note("capture \(n): the runner's observation of \(record.bundleId), \(record.built.nodeCount) nodes, snapshot \(record.snapshotMs) ms -> \(name)")
    }

    /// Performs `command` through the runner, as the agent's commands go (with `expect` when
    /// `bundleId` is given), and records the result.
    @MainActor
    @discardableResult
    func runnerDo(_ runner: XCUITestPhoneRunner, _ command: RunnerCommand, expecting bundleId: String?, _ what: String) async -> RunnerResult? {
        let expect = bundleId.map { CommandExpect(bundleId: $0) }
        do {
            let result = try await runner.perform(command, expect: expect)
            let detail = result.ok ? "" : ", failure \(String(describing: result.failure)), \(result.error ?? "")"
            note("runner \(what): \(Transcript.describe(command)), expect \(bundleId ?? "none") -> ok \(result.ok)\(detail)")
            return result
        } catch {
            note("runner \(what): \(Transcript.describe(command)) threw \(error)")
            return nil
        }
    }

    /// The drag the core plans for a swipe in `direction` ("left" or "right") on `frame`, or
    /// on the whole screen when nil (`jarvis-safety` gestures.rs: the safe area leaves 8 % at
    /// the top, 10 % at the bottom and 4 % on each side; the finger travels the middle 60 % of
    /// the region, at least 80 pt).
    static func coreSwipe(_ direction: String, frame: Rect?, screen: Size) -> (from: Point, to: Point)? {
        guard screen.width > 0, screen.height > 0 else { return nil }
        let safe = Rect(x: screen.width * 0.04, y: screen.height * 0.08, width: screen.width * 0.92, height: screen.height * 0.82)
        var region = safe
        if let frame {
            guard let clipped = intersection(frame, safe) else { return nil }
            region = clipped
        }
        let y = region.y + region.height / 2
        let centre = region.x + region.width / 2
        let half = max(region.width * 0.6 / 2, 40)
        let left = max(centre - half, safe.x)
        let right = min(centre + half, safe.x + safe.width)
        if direction == "left" {
            return (Point(x: right, y: y), Point(x: left, y: y))
        }
        return (Point(x: left, y: y), Point(x: right, y: y))
    }

    static func intersection(_ a: Rect, _ b: Rect) -> Rect? {
        let x0 = max(a.x, b.x)
        let y0 = max(a.y, b.y)
        let x1 = min(a.x + a.width, b.x + b.width)
        let y1 = min(a.y + a.height, b.y + b.height)
        guard x1 > x0, y1 > y0 else { return nil }
        return Rect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
    }

    static func centre(_ f: Rect) -> (x: Double, y: Double) {
        (f.x + f.width / 2, f.y + f.height / 2)
    }

    // MARK: - Typed trees (what the core receives)

    static func findAll(_ root: UiNode, _ test: (UiNode) -> Bool) -> [UiNode] {
        var out: [UiNode] = []
        var stack = [root]
        while let node = stack.popLast() {
            if test(node) { out.append(node) }
            stack.append(contentsOf: node.children.reversed())
        }
        return out
    }

    /// Whether `node` or a node under it has a label, value or identifier containing `text`.
    static func subtreeMentions(_ node: UiNode, _ text: String) -> Bool {
        !findAll(node) { n in
            (n.label ?? "").localizedCaseInsensitiveContains(text) || (n.value ?? "").localizedCaseInsensitiveContains(text)
                || (n.identifier ?? "").localizedCaseInsensitiveContains(text)
        }.isEmpty
    }

    static func describeNode(_ n: UiNode) -> String {
        var s = UiTreeBuilder.roleName(n.role)
        if let label = n.label, !label.isEmpty { s += " label \"\(clip(label))\"" }
        if let id = n.identifier, !id.isEmpty { s += " id \(clip(id))" }
        if let value = n.value, !value.isEmpty { s += " value \"\(clip(value))\"" }
        if let placeholder = n.placeholder, !placeholder.isEmpty { s += " placeholder \"\(clip(placeholder))\"" }
        s += " frame " + rectText(n.frame)
        if !n.enabled { s += " disabled" }
        if n.selected { s += " selected" }
        if !n.hittable { s += " not-hittable" }
        if n.secure { s += " secure" }
        return s
    }

    /// Writes how many typed nodes of `root` match, and up to `limit` of them.
    @MainActor
    func typedReport(_ root: UiNode?, _ what: String, limit: Int = 10, _ test: (UiNode) -> Bool) {
        guard let root else {
            note("\(what): no typed tree")
            return
        }
        let found = Self.findAll(root, test)
        note("\(what): \(found.count)")
        for node in found.prefix(limit) { lines.append("        - " + Self.describeNode(node)) }
        if found.count > limit { lines.append("        - and \(found.count - limit) more") }
    }

    static func typedCount(_ root: UiNode) -> Int {
        var count = 0
        var stack = [root]
        while let node = stack.popLast() {
            count += 1
            stack.append(contentsOf: node.children)
        }
        return count
    }

    // MARK: - Raw trees

    struct Hit {
        var element: RawElement
        var ancestors: [RawElement]
    }

    /// Writes how many nodes of `tree` match, and up to `limit` of them described. Returns the
    /// matches.
    @MainActor
    @discardableResult
    func report(_ tree: RawElement?, _ what: String, limit: Int = 10, _ match: (RawElement) -> Bool) -> [Hit] {
        guard let tree else {
            note("\(what): no tree (the capture failed)")
            return []
        }
        let found = Self.hits(tree, match)
        note("\(what): \(found.count)")
        for hit in found.prefix(limit) {
            lines.append("        - " + Self.describe(hit, screen: tree.frame))
        }
        if found.count > limit { lines.append("        - and \(found.count - limit) more (see the tree file)") }
        return found
    }

    static func hits(_ root: RawElement, _ match: (RawElement) -> Bool) -> [Hit] {
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

    /// `Button(9) role Button label "Delete" id trash frame {…} on screen yes; inside a raw
    /// Sheet no; alert ancestor none; ancestors: Application > Window > …`.
    static func describe(_ hit: Hit, screen: Rect) -> String {
        let e = hit.element
        var s = "\(XCUIElementTypeCode.name(e.elementType))(\(e.elementType)) role \(UiTreeBuilder.roleName(XCUIElementTypeCode.role(e.elementType)))"
        if !e.label.isEmpty { s += " label \"\(clip(e.label))\"" }
        if !e.identifier.isEmpty { s += " id \(clip(e.identifier))" }
        if let value = e.value, !value.isEmpty { s += " value \"\(clip(value))\"" }
        if let placeholder = e.placeholder, !placeholder.isEmpty { s += " placeholder \"\(clip(placeholder))\"" }
        if !e.title.isEmpty { s += " title \"\(clip(e.title))\"" }
        s += " frame " + rectText(e.frame)
        s += " on screen " + (onScreen(e.frame, screen) ? "yes" : "no")
        if !e.isEnabled { s += " disabled" }
        if e.isSelected { s += " selected" }
        let sheet = hit.ancestors.contains { $0.elementType == XCUIElementTypeCode.sheet || $0.elementType == XCUIElementTypeCode.popover }
        s += "; inside a raw Sheet/Popover " + (sheet ? "yes" : "no")
        if let alert = hit.ancestors.last(where: { $0.elementType == XCUIElementTypeCode.alert || $0.elementType == XCUIElementTypeCode.dialog }) {
            s += "; alert ancestor \(XCUIElementTypeCode.name(alert.elementType))" + (alert.label.isEmpty ? "" : " \"\(clip(alert.label))\"")
        } else {
            s += "; alert ancestor none"
        }
        s += "; ancestors: " + hit.ancestors.map { a in
            a.identifier.isEmpty ? XCUIElementTypeCode.name(a.elementType) : "\(XCUIElementTypeCode.name(a.elementType))#\(clip(a.identifier))"
        }.joined(separator: " > ")
        return s
    }

    /// The hits, briefly, joined with "; " (`empty` when there are none).
    static func briefs(_ found: [Hit], empty: String = "none") -> String {
        if found.isEmpty { return empty }
        let parts: [String] = found.map { brief($0.element) }
        return parts.joined(separator: "; ")
    }

    /// The hits' raw type names (with an identifier when they have one), joined with ", ".
    static func typeNames(_ found: [Hit]) -> String {
        if found.isEmpty { return "none" }
        let parts: [String] = found.map { hit in
            let name = XCUIElementTypeCode.name(hit.element.elementType)
            return hit.element.identifier.isEmpty ? name : "\(name)#\(clip(hit.element.identifier))"
        }
        return parts.joined(separator: ", ")
    }

    /// The last `count` ancestors' raw type names, outermost first.
    static func ancestry(_ hit: Hit, _ count: Int) -> String {
        let parts: [String] = hit.ancestors.suffix(count).map { XCUIElementTypeCode.name($0.elementType) }
        return parts.joined(separator: " > ")
    }

    /// One short line for a fact: raw type, label, identifier, value.
    static func brief(_ e: RawElement) -> String {
        var s = XCUIElementTypeCode.name(e.elementType)
        s += " label \"\(clip(e.label))\""
        s += " id " + (e.identifier.isEmpty ? "-" : clip(e.identifier))
        if let value = e.value, !value.isEmpty { s += " value \"\(clip(value))\"" }
        if e.isSelected { s += " selected" }
        if !e.isEnabled { s += " disabled" }
        return s
    }

    static func onScreen(_ f: Rect, _ screen: Rect) -> Bool {
        guard f.width > 0, f.height > 0, screen.width > 0, screen.height > 0 else { return false }
        return f.x < screen.x + screen.width && f.x + f.width > screen.x
            && f.y < screen.y + screen.height && f.y + f.height > screen.y
    }

    static func clip(_ text: String) -> String {
        let flat = text.replacingOccurrences(of: "\n", with: " ")
        return flat.count > 140 ? String(flat.prefix(140)) + "…" : flat
    }

    /// Label, value or identifier contains `text` (case-insensitive).
    static func mentions(_ text: String) -> (RawElement) -> Bool {
        { e in
            e.label.localizedCaseInsensitiveContains(text) || (e.value ?? "").localizedCaseInsensitiveContains(text)
                || e.identifier.localizedCaseInsensitiveContains(text)
        }
    }

    static func isType(_ types: [UInt]) -> (RawElement) -> Bool {
        { e in types.contains(e.elementType) }
    }

    static func rectText(_ f: Rect) -> String {
        String(format: "{%.0f,%.0f %.0fx%.0f}", f.x, f.y, f.width, f.height)
    }

    static func frameText(_ r: CGRect) -> String {
        String(format: "{%.0f,%.0f %.0fx%.0f}", Double(r.origin.x), Double(r.origin.y), Double(r.width), Double(r.height))
    }

    static func yesNo(_ value: Bool) -> String {
        value ? "yes" : "no"
    }

    /// Modal-like nodes of a raw tree: Alert, Dialog, Sheet, Popover, and Other nodes whose
    /// identifier names a sheet, popover, dimming view or dismiss region.
    static func modalish(_ e: RawElement) -> Bool {
        if [XCUIElementTypeCode.alert, XCUIElementTypeCode.dialog, XCUIElementTypeCode.sheet, XCUIElementTypeCode.popover].contains(e.elementType) {
            return true
        }
        let id = e.identifier.lowercased()
        return ["sheet", "popover", "dimming", "dismiss", "alert", "dialog"].contains { id.contains($0) }
    }

    // MARK: - The keyboard

    /// The return-key identifiers the core's stricter rule would read (audit C1).
    static let returnKeyIds = ["return", "go", "google", "join", "next", "route", "search", "send", "yahoo", "done", "continue", "emergencycall", "emergency call"]

    /// The keyboard in `tree`: its Keyboard node(s), the input host (`inputView`), the Emoji /
    /// Dictate controls with where they sit against the Keyboard node, the nodes whose
    /// identifier is a return-key type, and the bottom-right key of the Keyboard node (where
    /// the Return key sits). Returns that key.
    @MainActor
    @discardableResult
    func reportKeyboard(_ tree: RawElement?, _ what: String) -> RawElement? {
        guard let tree else {
            note("\(what): keyboard: no tree (the capture failed)")
            return nil
        }
        let boards = report(tree, "\(what): Keyboard nodes") { $0.elementType == XCUIElementTypeCode.keyboard }
        report(tree, "\(what): nodes with identifier inputView (the input host)") { $0.identifier == "inputView" }
        let extras = report(tree, "\(what): Emoji, Dictate and keyboard-switch controls") { e in
            ["Emoji", "Dictate", "Next keyboard", "Next Keyboard"].contains(e.label) || e.identifier == "dictation"
        }
        report(tree, "\(what): nodes whose identifier is a return-key type (case-insensitive)") { e in
            Self.returnKeyIds.contains(e.identifier.lowercased())
        }
        guard let kb = boards.first?.element.frame else {
            note("\(what): no Keyboard node: the Return key is not located")
            return nil
        }
        let bottom = kb.y + kb.height
        for extra in extras {
            let f = extra.element.frame
            let inside = f.y >= kb.y - 1 && f.y + f.height <= bottom + 1
            let place = inside ? "inside" : "outside"
            note("\(what): \(Self.brief(extra.element)) sits \(place) the Keyboard node's frame \(Self.rectText(kb)) (its frame \(Self.rectText(f)))")
        }
        let keys = Self.hits(tree) { e in
            (e.elementType == XCUIElementTypeCode.button || e.elementType == XCUIElementTypeCode.key)
                && e.frame.width > 0 && e.frame.height > 0
                && e.frame.y >= kb.y - 1 && e.frame.y + e.frame.height <= bottom + 1
                && e.frame.x >= kb.x - 1 && e.frame.x + e.frame.width <= kb.x + kb.width + 1
                && !["Emoji", "Dictate"].contains(e.label) && e.identifier != "dictation"
        }
        let mids = keys.map { $0.element.frame.y + $0.element.frame.height / 2 }
        guard let lowest = mids.max() else {
            note("\(what): no key inside the Keyboard node's frame")
            return nil
        }
        let row = keys.filter { abs(($0.element.frame.y + $0.element.frame.height / 2) - lowest) < 10 }
        guard let key = row.max(by: { $0.element.frame.x < $1.element.frame.x }) else { return nil }
        note("\(what): the bottom-right key of the Keyboard node (the Return key's place): " + Self.describe(key, screen: tree.frame))
        return key.element
    }

    // MARK: - Shared readings

    /// The input host's layout: the Keyboard node, the `inputView` host, Emoji and Dictate.
    @MainActor
    func inputHostFacts(_ tree: RawElement, _ what: String) {
        let keyboard = Self.hits(tree) { $0.elementType == XCUIElementTypeCode.keyboard }.first?.element
        let host = Self.hits(tree) { $0.identifier == "inputView" }.first?.element
        let emoji = Self.hits(tree) { $0.label == "Emoji" }.first?.element
        let dictate = Self.hits(tree) { $0.label == "Dictate" || $0.identifier == "dictation" }.first?.element
        fact("\(what): Keyboard node \(keyboard.map { Self.rectText($0.frame) } ?? "none"); input host (id inputView) \(host.map { Self.rectText($0.frame) } ?? "none"); Emoji \(emoji.map { Self.rectText($0.frame) } ?? "none"); Dictate \(dictate.map { Self.brief($0) + " " + Self.rectText($0.frame) } ?? "none")")
        if let keyboard, let emoji {
            fact("\(what): Emoji lies below the Keyboard node's bottom edge: \(Self.yesNo(emoji.frame.y >= keyboard.frame.y + keyboard.frame.height - 1))")
        }
    }

    static func pointText(_ p: Point) -> String {
        String(format: "(%.0f, %.0f)", p.x, p.y)
    }

    /// The frame of the smallest typed cell holding `text`, else the full-width band of the
    /// first node labelled with it.
    static func rowFrame(_ root: UiNode, _ text: String, _ screen: Size) -> Rect? {
        let cells = findAll(root) { $0.role == .cell && subtreeMentions($0, text) }
        if let cell = cells.min(by: { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }) { return cell.frame }
        if let node = findAll(root, { ($0.label ?? "").contains(text) }).first {
            return Rect(x: 0, y: node.frame.y, width: screen.width, height: node.frame.height)
        }
        return nil
    }

    /// Buttons inside a raw Alert node, briefly.
    static func alertButtons(_ tree: RawElement) -> [String] {
        hits(tree) { $0.elementType == XCUIElementTypeCode.button }
            .filter { hit in hit.ancestors.contains { $0.elementType == XCUIElementTypeCode.alert } }
            .map { brief($0.element) }
    }

    // MARK: - Driving helpers

    /// Opens the Playground's "Fact probes 3" row, then the probe screen `id`.
    @MainActor
    func openProbe3(_ app: XCUIApplication, _ id: String) async -> Bool {
        let entry = app.buttons["playground.probes3"].firstMatch
        guard waitFor(entry, "the Playground's Fact probes 3 row", 10) else { return false }
        tolerant("open Fact probes 3") { entry.tap() }
        let row = app.buttons[id].firstMatch
        guard waitFor(row, "the \(id) row", 5) else { return false }
        tolerant("open \(id)") { row.tap() }
        await pause(1)
        return true
    }

    /// Taps the first existing button among `labels` (identifier or label) in `app`. Returns
    /// the one tapped.
    @MainActor
    @discardableResult
    func tapFirstButton(_ app: XCUIApplication, _ labels: [String], _ what: String) -> String? {
        var tapped: String?
        tolerant(what) {
            for label in labels where tapped == nil {
                let button = app.buttons[label].firstMatch
                if button.exists {
                    button.tap()
                    tapped = label
                }
            }
        }
        note(tapped.map { "\(what): tapped \"\($0)\"" } ?? "\(what): none of \(labels) found; nothing tapped")
        return tapped
    }

    /// A tap at a normalised point of `app` (outside a menu or popover, to dismiss it).
    @MainActor
    func tapAt(_ app: XCUIApplication, _ dx: Double, _ dy: Double, _ what: String) {
        tolerant(what) { app.coordinate(withNormalizedOffset: CGVector(dx: dx, dy: dy)).tap() }
        note("\(what): tapped at \(Int(dx * 100))% of the width, \(Int(dy * 100))% of the height")
    }

    /// A vertical drag on `app` between two normalised heights at mid-width.
    @MainActor
    func drag(_ app: XCUIApplication, from: Double, to: Double, _ what: String) {
        tolerant(what) {
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: from))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: to))
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        note("\(what): dragged from \(Int(from * 100))% to \(Int(to * 100))% of the height")
    }

    /// Waits up to `timeout` s for an alert in any of `candidates`. Returns the first owner.
    @MainActor
    func waitForAlert(_ candidates: [(String, XCUIApplication)], _ timeout: TimeInterval) async -> (String, XCUIApplication)? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            for (id, app) in candidates {
                var up = false
                tolerant("look for an alert in \(id)") { up = app.alerts.firstMatch.exists }
                if up { return (id, app) }
            }
            await pause(0.3)
        }
        return nil
    }

    /// The first alert of `owner` by query: label, identifier, frame, buttons (label, id).
    /// Returns the button labels.
    @MainActor
    @discardableResult
    func readAlert(_ owner: XCUIApplication, _ ownerId: String, _ what: String) -> [String] {
        var labels: [String] = []
        tolerant("read the alert by query") {
            let alert = owner.alerts.firstMatch
            let buttons = alert.buttons.allElementsBoundByIndex
            labels = buttons.map { $0.label }
            let rows: [String] = buttons.map { b in
                let id = b.identifier.isEmpty ? "-" : b.identifier
                return "\"\(b.label)\" id \(id)"
            }
            let texts: [String] = alert.staticTexts.allElementsBoundByIndex.map { "\"\(Self.clip($0.label))\"" }
            let alertId = alert.identifier.isEmpty ? "-" : alert.identifier
            let textList = texts.joined(separator: ", ")
            let buttonList = rows.joined(separator: ", ")
            note("\(what): alert in \(ownerId) by query: label \"\(Self.clip(alert.label))\", id \(alertId), frame \(Self.frameText(alert.frame)); texts: \(textList); buttons: \(buttonList)")
        }
        return labels
    }

    /// The button of a permission prompt that refuses (Don't Allow, Ask App Not to Track, …).
    static func denyingLabel(_ labels: [String]) -> String? {
        for key in ["don", "not", "deny", "cancel", "close"] {
            if let hit = labels.first(where: { $0.lowercased().contains(key) }) { return hit }
        }
        return nil
    }

    /// Taps `label` in `owner`'s first alert and waits (≤ 4 s) for the alert to go.
    @MainActor
    func tapInAlert(_ owner: XCUIApplication, _ label: String, _ what: String) async {
        tolerant(what) {
            let button = owner.alerts.firstMatch.buttons[label].firstMatch
            if button.exists { button.tap() } else { note("\(what): no button \"\(label)\" in the alert") }
        }
        let deadline = Date().addingTimeInterval(4)
        while existsNow(owner.alerts.firstMatch), Date() < deadline { await pause(0.3) }
        note("\(what): tapped \"\(label)\"; alert still up afterwards: \(Self.yesNo(existsNow(owner.alerts.firstMatch)))")
    }

    /// Opens `url` in Safari from the Home Screen and waits (≤ 30 s) for a node labelled
    /// `heading`; closes Safari's first-launch tip if it is up. Returns Safari, or nil.
    @MainActor
    func openInSafari(_ url: String, waitFor heading: String) async -> XCUIApplication? {
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
            return nil
        }
        let text = Self.node(in: safari, labelled: heading)
        note(waitFor(text, "the page heading \"\(heading)\"", 30)
             ? "the page heading \"\(heading)\" is in Safari's tree"
             : "the page heading \"\(heading)\" did not appear in Safari's tree within 30 s")
        let tip = safari.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "You can now view these items")).firstMatch
        if existsNow(tip) {
            tolerant("close Safari's tip") {
                let close = safari.buttons["Close"].firstMatch
                if close.exists { close.tap() }
            }
            await pause(1)
        }
        return safari
    }

    /// The first node of any type whose label is exactly `text`.
    @MainActor
    static func node(in app: XCUIApplication, labelled text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label == %@", text)).firstMatch
    }

    /// The first node of any type whose label contains `text`.
    @MainActor
    static func node(in app: XCUIApplication, containing text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", text)).firstMatch
    }

    @MainActor
    func waitFor(_ element: XCUIElement, _ what: String, _ timeout: TimeInterval) -> Bool {
        var found = false
        tolerant("wait for \(what)") { found = element.waitForExistence(timeout: timeout) }
        if !found { note("\(what): not found within \(timeout.compactText) s") }
        return found
    }

    @MainActor
    func existsNow(_ element: XCUIElement) -> Bool {
        var found = false
        tolerant("check existence") { found = element.exists }
        return found
    }

    /// `exists && isHittable` by query, read tolerantly (XCUITest's own "is it covered" view).
    @MainActor
    func hittableNow(_ element: XCUIElement) -> Bool {
        var hittable = false
        tolerant("check hittability") { hittable = element.exists && element.isHittable }
        return hittable
    }

    /// A label read by query ("(none)" when the element is not there).
    @MainActor
    func labelNow(_ element: XCUIElement) -> String {
        var text = "(none)"
        tolerant("read a label") { if element.exists { text = element.label } }
        return text
    }

    /// The frame XCUITest's query reports for `element`, nil when it does not exist.
    @MainActor
    func queryFrame(_ element: XCUIElement) -> CGRect? {
        var frame: CGRect?
        tolerant("read a frame by query") {
            if element.exists { frame = element.frame }
        }
        return frame
    }

    @MainActor
    func isRunning(_ app: XCUIApplication) -> Bool {
        var running = false
        tolerant("read an app state") {
            let state = app.state
            running = state == .runningForeground || state == .runningBackground
        }
        return running
    }

    @MainActor
    func stateText(_ app: XCUIApplication) -> String {
        var text = "?"
        tolerant("read an app state") { text = "\(app.state.rawValue)" }
        return text
    }

    func pause(_ seconds: Double) async {
        try? await Task.sleep(for: .milliseconds(Int(seconds * 1_000)))
    }

    /// Runs one probing action; XCTest issues it raises are captured as data (the lock probe's
    /// pattern). Returns whether it ran without an issue or a throw.
    @MainActor
    @discardableResult
    func tolerant(_ what: String, _ body: () throws -> Void) -> Bool {
        var issues: [String] = []
        let options = XCTExpectedFailure.Options()
        options.isStrict = false
        options.issueMatcher = { issue in
            issues.append(issue.compactDescription)
            return true
        }
        var ok = true
        do {
            _ = try XCTExpectFailure("probe round 3: \(what) may fail; recorded, not judged", options: options) { () throws -> Bool in
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
}

/// A minimal HTTP/1.1 server on 127.0.0.1 for probe round 3 (the shape of JarvisKit's
/// `FormCaptureServer`, which this round cannot change): `GET /auth…` answers 401 with an HTTP
/// Basic challenge (realm "Probe realm") unless an Authorization header came, `POST /submit`
/// records the form, every other `GET` serves `page`. Every request is logged. Nothing leaves
/// the simulator's loopback.
final class Probe3Server: Sendable {
    let port: UInt16
    let page: String
    private let listenFD: Int32
    private let submitted = Mutex<[[String: String]]>([])
    private let logged = Mutex<[String]>([])
    private let stopped = Mutex(false)

    var baseURL: String { "http://127.0.0.1:\(port)" }
    var submissions: [[String: String]] { submitted.withLock { $0 } }
    var log: [String] { logged.withLock { $0 } }

    enum ServerError: Error { case socket(String) }

    init(page: String) throws {
        self.page = page
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { throw ServerError.socket("socket() failed") }
        var yes: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = in_port_t(0).bigEndian
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0, listen(fd, 64) == 0 else {
            close(fd)
            throw ServerError.socket("bind/listen on 127.0.0.1 failed (errno \(errno))")
        }
        var actual = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        _ = withUnsafeMutablePointer(to: &actual) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(fd, $0, &length) }
        }
        self.port = UInt16(bigEndian: actual.sin_port)
        self.listenFD = fd
        Thread.detachNewThread { [self] in acceptLoop() }
    }

    func stop() {
        let wasStopped = stopped.withLock { s -> Bool in
            defer { s = true }
            return s
        }
        guard !wasStopped else { return }
        shutdown(listenFD, Int32(SHUT_RDWR))
    }

    deinit { stop() }

    private func acceptLoop() {
        while true {
            if stopped.withLock({ $0 }) {
                close(listenFD)
                return
            }
            var pfd = pollfd(fd: listenFD, events: Int16(POLLIN), revents: 0)
            guard poll(&pfd, 1, 50) > 0 else { continue }
            if stopped.withLock({ $0 }) { continue }
            let client = accept(listenFD, nil, nil)
            if client < 0 {
                usleep(10_000)
                continue
            }
            var one: Int32 = 1
            setsockopt(client, SOL_SOCKET, SO_NOSIGPIPE, &one, socklen_t(MemoryLayout<Int32>.size))
            Thread.detachNewThread { [self] in serve(client) }
        }
    }

    private func serve(_ fd: Int32) {
        defer {
            shutdown(fd, Int32(SHUT_RDWR))
            close(fd)
        }
        guard let request = readRequest(fd) else {
            logged.withLock { $0.append("unparseable request") }
            writeAll(fd, Data("HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".utf8))
            return
        }
        var status = "200 OK"
        var extra = ""
        var body = ""
        if request.method == "POST" && request.path.hasPrefix("/submit") {
            let fields = FormCaptureServer.parseForm(request.body)
            submitted.withLock { $0.append(fields) }
            logged.withLock { $0.append("POST \(request.path) " + fields.keys.sorted().map { "\($0)=\(fields[$0] ?? "")" }.joined(separator: " ")) }
            body = "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Received</title></head><body style=\"font: 18px -apple-system, sans-serif; margin: 20px;\"><h1>Probe form received</h1></body></html>"
        } else if request.method == "GET" && request.path.hasPrefix("/auth") {
            let authorized = request.headers["authorization"] != nil
            logged.withLock { $0.append("GET \(request.path) authorization header " + (authorized ? "present" : "absent")) }
            if authorized {
                body = "<!doctype html><html><body><h1>Probe signed in</h1></body></html>"
            } else {
                status = "401 Unauthorized"
                extra = "WWW-Authenticate: Basic realm=\"Probe realm\"\r\n"
                body = "<!doctype html><html><body><h1>Probe auth required</h1></body></html>"
            }
        } else if request.method == "GET" {
            logged.withLock { $0.append("GET \(request.path) (user-agent: \(request.headers["user-agent"] ?? "-"))") }
            if request.path.hasPrefix("/favicon") {
                status = "404 Not Found"
            } else {
                body = page
            }
        } else {
            logged.withLock { $0.append("\(request.method) \(request.path) (not handled)") }
            status = "405 Method Not Allowed"
        }
        let data = Data(body.utf8)
        var head = "HTTP/1.1 \(status)\r\n"
        head += "Content-Type: text/html; charset=utf-8\r\n"
        head += "Cache-Control: no-store\r\n"
        head += extra
        head += "Content-Length: \(data.count)\r\n"
        head += "Connection: close\r\n\r\n"
        writeAll(fd, Data(head.utf8) + data)
    }

    private struct Request {
        var method: String
        var path: String
        var headers: [String: String]
        var body: String
    }

    private func readRequest(_ fd: Int32) -> Request? {
        var buffer = Data()
        let separator = Data("\r\n\r\n".utf8)
        var chunk = [UInt8](repeating: 0, count: 16 * 1024)
        var headerEnd: Range<Data.Index>?
        while headerEnd == nil {
            let n = recv(fd, &chunk, chunk.count, 0)
            guard n > 0 else { return nil }
            buffer.append(contentsOf: chunk[0..<n])
            headerEnd = buffer.range(of: separator)
            if buffer.count > 256 * 1024 { return nil }
        }
        guard let headerEnd else { return nil }
        let headText = String(decoding: buffer[buffer.startIndex..<headerEnd.lowerBound], as: UTF8.self)
        var headLines = headText.components(separatedBy: "\r\n")
        guard !headLines.isEmpty else { return nil }
        let requestLine = headLines.removeFirst().split(separator: " ")
        guard requestLine.count >= 2 else { return nil }
        var headers: [String: String] = [:]
        for line in headLines {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let name = line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            headers[name] = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        }
        let length = min(Int(headers["content-length"] ?? "0") ?? 0, 1_000_000)
        var body = Data(buffer[headerEnd.upperBound...])
        while body.count < length {
            let n = recv(fd, &chunk, min(chunk.count, length - body.count), 0)
            guard n > 0 else { return nil }
            body.append(contentsOf: chunk[0..<n])
        }
        return Request(method: String(requestLine[0]), path: String(requestLine[1]), headers: headers,
                       body: String(decoding: body, as: UTF8.self))
    }

    private func writeAll(_ fd: Int32, _ data: Data) {
        data.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return }
            var sent = 0
            while sent < raw.count {
                let n = send(fd, base + sent, raw.count - sent, 0)
                if n <= 0 { return }
                sent += n
            }
        }
    }
}
