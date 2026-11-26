import Foundation
import Synchronization
import UIKit
import XCTest
import JarvisCore
import JarvisKit

/// Probe round 3, on the Playground's own fixture screens ("Fact probes 3",
/// `ProbeFixtures3.swift`): facts the safety gate assumes but no real iOS 26.5 tree had shown
/// (docs/ARCHITECTURE.md, *What real trees showed*; the audit's P2, P3, P4, P9, P10, P11), and
/// P13 (row swipe lengths), P13e-g (a second swipe or a tap on a row whose actions are revealed,
/// and the tree while they are) and P14 (the runner under a hung front app).
/// Conventions: `Probe3Case`. Each test RECORDS; it fails only when its own set-up cannot run
/// (the app does not launch) and skips, after attaching the evidence, when a state cannot be
/// produced on a simulator.
///
/// Simulator time (typical, run 139 where measured): P2 ~115 s, P3 ~35 s, P4a ~25 s,
/// P4b ~105 s, P4c ~115 s, P4d ~85 s, P9a ~70 s, P9b ~80 s, P10 ~50 s, P11a ~105 s,
/// P11b ~55 s, P13 ~130 s, P14 ~100 s; P13e ~190 s, P13f ~50 s, P13g ~35 s (estimated, not yet
/// measured).
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
        /// Open with a coordinate tap at the trigger's frame centre (the SwiftUI Menu did not
        /// open from `tap()` in run 139).
        var coordinateTap = false
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
                  destructive: "Delete from menu", dismiss: [], coordinateTap: true),
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
            // Let the previous modal's dismissal finish before the next trigger.
            await pause(0.8)
            let marker = Self.node(in: app, labelled: item.marker)
            var opened = false
            if item.longPress {
                tolerant("long-press for the \(item.name)") { trigger.press(forDuration: 1.2) }
                await pause(1.5)
                opened = existsNow(marker)
            } else if item.coordinateTap {
                opened = await openMenu(app, trigger, marker, item.name)
            } else {
                tolerant("open the \(item.name)") { trigger.tap() }
                await pause(1.5)
                opened = existsNow(marker)
            }
            if !opened, !item.longPress, !item.coordinateTap, hittableNow(trigger) {
                note("\(item.name): its marker \"\(item.marker)\" is not there 1.5 s after the tap and the trigger is still hittable: one more tap at its frame centre")
                tapCentre(app, trigger, "open the \(item.name) again (tap at its frame centre)")
                await pause(1.5)
                opened = existsNow(marker)
            }
            note("\(item.name): its marker \"\(item.marker)\" found by query: \(Self.yesNo(opened))")
            fact("\(item.name): opened (its marker \"\(item.marker)\" found by query): \(Self.yesNo(opened))")
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

    /// Opens the SwiftUI Menu without touching any of its elements (run 149: XCUITest's tap on
    /// the Menu's label failed "Not hittable … hit point {-1, -1} after scrolling to visible"
    /// and ended the test body). The Menu's frame comes from a snapshot (the fixture now puts
    /// it above the Form, outside any List row); if its centre is outside the band a tap may
    /// use, the screen is dragged to bring it to the middle. Then, in turn: a coordinate tap at
    /// its centre in Jarvis's coordinate space, the same through SpringBoard's, a 0.8 s press
    /// there. After each, up to 2.5 s for the menu's items in a snapshot. Records which way
    /// opened it.
    @MainActor
    private func openMenu(_ app: XCUIApplication, _ trigger: XCUIElement, _ marker: XCUIElement, _ name: String) async -> Bool {
        guard let raw = snapshotTree(app, "Jarvis (the Menu's frame)"),
              let hit = Self.hits(raw, { $0.identifier == "probe3.modals.menu" }).first
                ?? Self.hits(raw, { $0.elementType == XCUIElementTypeCode.button && $0.label == "Probe menu" }).first else {
            fact("\(name): no node for the Menu (id probe3.modals.menu or a Button \"Probe menu\") in Jarvis's snapshot")
            return false
        }
        var frame = hit.element.frame
        let screen = raw.frame
        fact("\(name): the Menu's node: \(Self.brief(hit.element)) \(Self.rectText(frame)), ancestors \(Self.ancestry(hit, 5))")
        let midY = frame.y + frame.height / 2
        if midY < screen.height * 0.12 || midY > screen.height * 0.85 {
            let to = screen.height * 0.5
            tolerant("drag the Menu's row to the middle of the screen") {
                let origin = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
                origin.withOffset(CGVector(dx: screen.width * 0.5, dy: min(max(midY, screen.height * 0.15), screen.height * 0.85)))
                    .press(forDuration: 0.05, thenDragTo: origin.withOffset(CGVector(dx: screen.width * 0.5, dy: to)))
            }
            await pause(1)
            if let again = snapshotTree(app, "Jarvis after the drag"), let moved = Self.hits(again, { $0.identifier == "probe3.modals.menu" }).first {
                frame = moved.element.frame
            }
            note("\(name): the Menu's frame after the drag \(Self.rectText(frame))")
        }
        let centre = CGVector(dx: frame.x + frame.width / 2, dy: frame.y + frame.height / 2)
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        var ways: [String] = []
        for way in ["a coordinate tap at its centre (Jarvis's coordinates)", "a coordinate tap at its centre (SpringBoard's coordinates)", "a 0.8 s press at its centre"] {
            tolerant("open the \(name): \(way)") {
                let space = way.contains("SpringBoard") ? board : app
                let point = space.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0)).withOffset(centre)
                if way.contains("press") { point.press(forDuration: 0.8) } else { point.tap() }
            }
            var up = false
            let deadline = Date().addingTimeInterval(2.5)
            while !up, Date() < deadline {
                await pause(0.5)
                up = snapshotTree(app, "Jarvis (is the menu open?)").map { tree in !Self.hits(tree) { $0.label == "Probe menu item" || $0.label == "Delete from menu" }.isEmpty } ?? false
            }
            ways.append("\(way): \(up ? "opened" : "not opened")")
            if up {
                fact("\(name): opening attempts: \(ways.joined(separator: "; "))")
                return true
            }
            await pause(0.5)
        }
        fact("\(name): opening attempts: \(ways.joined(separator: "; "))")
        return false
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
            // The row's own text only: the status line ("Deleted swipe row 1") mentions it too.
            let still = after.map { tree in !Self.hits(tree) { $0.label == "Swipe row 1" }.isEmpty } ?? false
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
        // The row's own text node only (run 139 matched the status "Deleted swipe row 2" and
        // reported the deleted row as still there).
        let still = after.map { tree in !Self.hits(tree) { $0.label == "Swipe row 2" }.isEmpty } ?? false
        fact("runner full-width swipe left (\(Self.pointText(from)) -> \(Self.pointText(to))): row 2 (a node labelled exactly \"Swipe row 2\") still in the tree \(Self.yesNo(still)); status \"\(labelNow(status))\"")
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

    private typealias PermissionCase = (id: String, slug: String, name: String)

    /// Prompts about the device (P9a) and about the owner's data (P9b): two tests, each well
    /// inside its allowance (run 139 ran all eight in one test and timed out in the paste case).
    private static let devicePermissionCases: [PermissionCase] = [
        ("probe3.perm.location", "location", "location when in use"),
        ("probe3.perm.camera", "camera", "camera"),
        ("probe3.perm.microphone", "microphone", "microphone"),
        ("probe3.perm.contacts", "contacts", "contacts"),
    ]
    private static let dataPermissionCases: [PermissionCase] = [
        // Photos last: in runs 141, 142 and 146 the test ended right after the photo prompt
        // was read (see `readAlert`); whatever happens there, the others are recorded first.
        ("probe3.perm.tracking", "tracking", "tracking transparency"),
        ("probe3.perm.localnet", "localnet", "local network"),
        ("probe3.perm.paste", "paste", "paste (pasteboard written by the test runner)"),
        ("probe3.perm.photos", "photos", "photo library"),
    ]

    @MainActor
    func testP09a_PermissionPromptsDevice() async throws {
        try await permissionProbe("p9a-permissions", Self.devicePermissionCases,
                                  "For the location, camera, microphone and contacts prompts raised from the Playground: is each an Alert in SpringBoard's tree only, what are its buttons' labels and identifiers, does the runner's top-layer reading see it? (Each is denied.)")
    }

    @MainActor
    func testP09b_PermissionPromptsData() async throws {
        try await permissionProbe("p9b-permissions", Self.dataPermissionCases,
                                  "For the photo library, tracking, local network and paste prompts raised from the Playground: is each an Alert in SpringBoard's tree only, its buttons, does the runner's top-layer reading see it? With the paste prompt up (Jarvis's main thread blocked in the read), does the runner report SpringBoard's alert without touching Jarvis, and how long does its look take? (Each is denied.)")
    }

    @MainActor
    private func permissionProbe(_ name: String, _ cases: [PermissionCase], _ question: String) async throws {
        executionTimeAllowance = 180
        begin(name, question)
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.permissions") {
            await probePermissions(app, cases)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the Permissions screen not reached?")
            reason = "the Permissions screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    /// Per prompt: ask, wait (≤ 10 s) for an alert, SpringBoard's tree, the alert by query,
    /// the runner's look (also when no alert was found: run 139 had none for photos), Jarvis's
    /// tree, deny. The paste prompt has its own order (`probePaste`).
    @MainActor
    private func probePermissions(_ app: XCUIApplication, _ cases: [PermissionCase]) async {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let status = app.staticTexts["probe3.perm.status"].firstMatch
        for item in cases {
            if item.slug == "paste" {
                await probePaste(app, board, status, item)
                continue
            }
            let button = app.buttons[item.id].firstMatch
            guard waitFor(button, "the \(item.name) button", 5) else {
                fact("\(item.name): could not produce (no button)")
                continue
            }
            tolerant("ask for \(item.name)") { button.tap() }
            var owner = await waitForAlert([(Self.springboardId, board), (Self.appBundleId, app)], 10)
            await pause(0.5)
            let boardTree = capture(board, Self.springboardId, "\(item.slug)-springboard", "After asking for \(item.name): is the prompt an Alert in SpringBoard's tree? Its buttons' labels and identifiers?")
            if owner == nil, existsNow(board.alerts.firstMatch) {
                note("\(item.name): an alert came up in SpringBoard after the 10 s wait")
                owner = (Self.springboardId, board)
            }
            var labels: [String] = []
            if let (ownerId, ownerApp) = owner { labels = readAlert(ownerApp, ownerId, item.name) }
            let moment = owner == nil ? "after asking for \(item.name) (no alert found by query)" : "the \(item.name) prompt up"
            let look = await runnerLook(moment, item.slug)
            let appTree = capture(app, Self.appBundleId, "\(item.slug)-jarvis", "After asking for \(item.name): does Jarvis's own tree hold the prompt?")
            let inBoard = boardTree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.alert }.count } ?? 0
            let inApp = appTree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.alert }.count } ?? 0
            let buttons = (boardTree.map { Self.alertButtons($0) } ?? []) + (appTree.map { Self.alertButtons($0) } ?? [])
            let lookApp = look?.appBundleId ?? "?"
            guard let (ownerId, ownerApp) = owner else {
                fact("\(item.name): no Alert in SpringBoard's or Jarvis's tree within 10 s (Alert nodes: SpringBoard \(inBoard), Jarvis \(inApp)); the runner's top layer \(lastRunnerFront), its look of \(lookApp); status \"\(labelNow(status))\"")
                await pause(0.5)
                continue
            }
            let deny = Self.denyingLabel(labels) ?? labels.last
            let quoted: [String] = labels.map { "\"\($0)\"" }
            let buttonText = buttons.isEmpty ? quoted.joined(separator: ", ") : buttons.joined(separator: "; ")
            let denyText = deny ?? "nothing to tap"
            fact("\(item.name): the prompt is an Alert in \(ownerId)'s tree by query; Alert nodes in the snapshots: SpringBoard \(inBoard), Jarvis \(inApp); buttons: \(buttonText); the runner's top layer: \(lastRunnerFront), its look of \(lookApp); denied with \"\(denyText)\"")
            if let deny, existsNow(ownerApp.alerts.firstMatch) {
                await tapInAlert(ownerApp, deny, "deny the \(item.name) prompt")
            } else if deny != nil {
                note("\(item.name): the alert was gone before the denial (handled by XCTest's interruption handling while Jarvis was captured?)")
            }
            await pause(0.8)
            note("\(item.name): status afterwards \"\(labelNow(status))\"")
        }
    }

    /// The paste prompt. Jarvis reads the pasteboard on its main thread, which blocks until
    /// the prompt is answered (run 139: XCUITest's own tap then waited 60 s for Jarvis to
    /// idle, and the probe's snapshot of Jarvis retried until the allowance ran out). So:
    /// the frame of Read pasteboard is read while Jarvis is idle, the tap goes through
    /// SpringBoard's coordinates, and until the prompt is answered only SpringBoard and the
    /// runner's look are touched (the runner checks SpringBoard's alerts first; whether it
    /// reports the alert without touching Jarvis, and how long its look takes, is recorded).
    /// Jarvis is captured only after the prompt is answered.
    @MainActor
    private func probePaste(_ app: XCUIApplication, _ board: XCUIApplication, _ status: XCUIElement, _ item: PermissionCase) async {
        let button = app.buttons[item.id].firstMatch
        guard waitFor(button, "the \(item.name) button", 5), let frame = queryFrame(button) else {
            fact("\(item.name): could not produce (no Read pasteboard button)")
            return
        }
        UIPasteboard.general.string = "Probe paste 5521"
        note("the test runner (another process) wrote \"Probe paste 5521\" to the general pasteboard")
        note("from the tap until the prompt is answered nothing queries or captures Jarvis")
        checkpoint("tapping Read pasteboard (Jarvis's main thread blocks in the read until the paste prompt is answered)", force: true)
        tapThroughSpringBoard(frame, "tap Read pasteboard")
        let owner = await waitForAlert([(Self.springboardId, board)], 8)
        await pause(0.5)
        let boardTree = capture(board, Self.springboardId, "\(item.slug)-springboard", "After asking to read the pasteboard: is the paste prompt an Alert in SpringBoard's tree? Its buttons' labels and identifiers?")
        let inBoard = boardTree.map { Self.hits($0) { $0.elementType == XCUIElementTypeCode.alert }.count } ?? 0
        let buttons = boardTree.map { Self.alertButtons($0) } ?? []
        var labels: [String] = []
        if owner != nil { labels = readAlert(board, Self.springboardId, item.name) }
        checkpoint("the runner's look with the paste prompt up (Jarvis blocked)", force: true)
        let lookStart = Date()
        let look = await runnerLook("the paste prompt up, Jarvis's main thread blocked in the read", item.slug)
        let lookSeconds = String(format: "%.1f", Date().timeIntervalSince(lookStart))
        let lookApp = look?.appBundleId ?? "?"
        let lookAlerts = look?.root.map { root in Self.findAll(root) { $0.role == .alert }.count } ?? 0
        let deny = Self.denyingLabel(labels) ?? "Don’t Allow Paste"
        var answered = "nothing (no alert by query)"
        if existsNow(board.alerts.firstMatch) {
            await tapInAlert(board, deny, "answer the paste prompt")
            answered = "\"\(deny)\""
        }
        let quoted: [String] = labels.map { "\"\($0)\"" }
        let buttonText = buttons.isEmpty ? quoted.joined(separator: ", ") : buttons.joined(separator: "; ")
        let ownerText = owner == nil ? "no Alert in SpringBoard by query within 8 s" : "an Alert in SpringBoard's tree by query"
        fact("\(item.name): \(ownerText); Alert nodes in SpringBoard's snapshot \(inBoard); buttons: \(buttonText); with Jarvis blocked the runner's look took \(lookSeconds) s in all (frontBundleId() \(lastRunnerFrontMs) ms -> \(lastRunnerFront), snapshot \(lastRunnerSnapshotMs) ms -> of \(lookApp), \(lookAlerts) typed Alert node(s)); answered with \(answered)")
        await pause(1)
        capture(app, Self.appBundleId, "\(item.slug)-jarvis-after", "After the paste prompt was answered: is Jarvis responsive again; what did the read return (the status line)?")
        note("\(item.name): status afterwards \"\(labelNow(status))\"")
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
        // The status line keeps its history, newest first ("Provider reset (was: Reporting
        // call)"), so the order of CallKit's answers is recorded; decisions read the newest.
        var shown = ""
        let deadline = Date().addingTimeInterval(10)
        while Date() < deadline {
            shown = labelNow(status)
            if shown.hasPrefix("Call report failed") || shown.hasPrefix("Call reported") { break }
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

    // MARK: - P13: row swipe lengths

    /// One drag of P13. Its length is the core's (no share, no points: the core's swipe, or
    /// its horizontal scroll when `scroll` is set), `screenShare` of the screen width,
    /// `rowShare` of the row's width, or `points`; centred on the row. Through the runner
    /// (which sends XCTest's own drag speed whatever the duration) or as an XCUITest drag at
    /// velocity length / `ms`. `rightward`: the finger moves left to right (leading actions).
    private struct SwipeAttempt {
        var name: String
        var screenShare: Double?
        var rowShare: Double?
        var points: Double?
        var runner: Bool
        var scroll: String?
        var ms: UInt64
        var rightward: Bool

        init(_ name: String, screenShare: Double? = nil, rowShare: Double? = nil, points: Double? = nil,
             runner: Bool = false, scroll: String? = nil, ms: UInt64 = 200, rightward: Bool = false) {
            self.name = name
            self.screenShare = screenShare
            self.rowShare = rowShare
            self.points = points
            self.runner = runner
            self.scroll = scroll
            self.ms = ms
            self.rightward = rightward
        }
    }

    /// One row kind of P13: its screen, its rows' text, its actions (the one that removes the
    /// row first) and the drags made on it.
    private struct SwipeSet {
        var id: String
        var set: String
        var prefix: String
        var statusId: String
        var actions: [String]
        var attempts: [SwipeAttempt]
    }

    /// Runs 146 and 149 on the 400 pt rows of the Swipe rows screen: 240 pt revealed, 264 pt
    /// deleted; the runner's and the XCUITest 60-90 %-of-screen drags agreed twice (only
    /// the 60 % one is kept, as the reference).
    private static let fullWidthSet = SwipeSet(
        id: "probe3.swipes", set: "full width (400 pt rows)", prefix: "Full swipe row", statusId: "probe3.swipes.status",
        actions: ["Delete", "Flag"],
        attempts: [
            SwipeAttempt("the core's swipe left (runner)", runner: true),
            SwipeAttempt("the core's scroll right, finger left (runner)", runner: true, scroll: "right"),
            SwipeAttempt("the core's scroll left, finger right (runner)", runner: true, scroll: "left"),
            SwipeAttempt("60 % of the screen (XCUITest drag, 200 ms)", screenShare: 0.6),
            SwipeAttempt("40 pt (XCUITest drag, 200 ms)", points: 40),
            SwipeAttempt("60 pt (XCUITest drag, 200 ms)", points: 60),
            SwipeAttempt("80 pt (XCUITest drag, 200 ms)", points: 80),
            SwipeAttempt("100 pt (XCUITest drag, 200 ms)", points: 100),
            SwipeAttempt("120 pt (XCUITest drag, 200 ms)", points: 120),
            SwipeAttempt("160 pt (XCUITest drag, 200 ms)", points: 160),
            SwipeAttempt("200 pt (XCUITest drag, 200 ms)", points: 200),
            SwipeAttempt("220 pt (XCUITest drag, 200 ms)", points: 220),
            SwipeAttempt("250 pt (XCUITest drag, 200 ms)", points: 250),
            SwipeAttempt("255 pt (XCUITest drag, 200 ms)", points: 255),
            SwipeAttempt("260 pt (XCUITest drag, 200 ms)", points: 260),
            SwipeAttempt("264 pt (XCUITest drag, 200 ms)", points: 264),
            SwipeAttempt("50 % of the row (XCUITest drag, 200 ms)", rowShare: 0.5),
            SwipeAttempt("50 % of the row (XCUITest drag, 800 ms)", rowShare: 0.5, ms: 800),
            SwipeAttempt("66 % of the row (XCUITest drag, 200 ms)", rowShare: 0.66),
            SwipeAttempt("66 % of the row (XCUITest drag, 800 ms)", rowShare: 0.66, ms: 800),
        ])

    private static func shareAttempts(_ shares: [Double], rightward: Bool) -> [SwipeAttempt] {
        shares.map { share in
            SwipeAttempt("\(Int((share * 100).rounded())) % of the row\(rightward ? ", left to right" : "") (XCUITest drag, 200 ms)", rowShare: share, rightward: rightward)
        }
    }

    /// Run 149: narrow and column rows revealed at 40-66 % of the row and deleted at 75 %.
    private static let narrowSet = SwipeSet(
        id: "probe3.swipesNarrow", set: "a 300 pt list", prefix: "Narrow row", statusId: "probe3.swipevariant.status",
        actions: ["Delete", "Flag"], attempts: shareAttempts([0.66, 0.69, 0.72, 0.75], rightward: false))
    private static let columnsSet = SwipeSet(
        id: "probe3.swipesColumns", set: "the left of two 200 pt columns", prefix: "Left row", statusId: "probe3.swipevariant.status",
        actions: ["Delete", "Flag"], attempts: shareAttempts([0.66, 0.69, 0.72, 0.75], rightward: false))
    /// Run 149: leading-action 440 pt rows revealed at 40-55 % and deleted at 60 %.
    private static let leadingSet = SwipeSet(
        id: "probe3.swipesLeading", set: "a leading action, left to right (440 pt rows)", prefix: "Leading row", statusId: "probe3.swipevariant.status",
        actions: ["Remove", "Pin"],
        attempts: shareAttempts([0.5, 0.55, 0.57, 0.6], rightward: true) + [SwipeAttempt("the core's swipe right (runner)", runner: true, rightward: true)])
    private static let plainSet = SwipeSet(
        id: "probe3.swipesPlain", set: "a trailing action, edge to edge (440 pt rows)", prefix: "Plain row", statusId: "probe3.swipevariant.status",
        actions: ["Delete", "Flag"], attempts: shareAttempts([0.5, 0.55, 0.57, 0.6], rightward: false))

    @MainActor
    func testP13a_RowSwipeLengthsFullWidth() async throws {
        try await swipeProbe("p13a-swipe-lengths", [Self.fullWidthSet], allowance: 200,
                             "On full-width 400 pt list rows whose trailing actions allow a full swipe (Delete, Flag): what do the core's swipe and scrolls, and drags of 40-264 pt and of 50 % and 66 % of the row at 200 and 800 ms do: reveal, delete, or nothing? Does anything reveal below 120 pt; where between 250 and 264 pt does reveal turn into delete; does velocity matter?")
    }

    @MainActor
    func testP13b_RowSwipeLengthsNarrow() async throws {
        try await swipeProbe("p13b-swipe-narrow", [Self.narrowSet], allowance: 150,
                             "On the rows of a 300 pt list (trailing Delete, Flag; full swipe allowed): do drags of 66, 69, 72 and 75 % of the ROW width reveal or delete?")
    }

    @MainActor
    func testP13c_RowSwipeLengthsColumns() async throws {
        try await swipeProbe("p13c-swipe-columns", [Self.columnsSet], allowance: 150,
                             "On the rows of the left of two 200 pt columns (trailing Delete, Flag; full swipe allowed): do drags of 66, 69, 72 and 75 % of the ROW width reveal or delete?")
    }

    @MainActor
    func testP13d_RowSwipeLengthsLeadingAndEdgeToEdge() async throws {
        try await swipeProbe("p13d-swipe-leading", [Self.leadingSet, Self.plainSet], allowance: 150,
                             "On 440 pt edge-to-edge rows: with a leading full-swipe action (Remove, Pin), left to right, and with a trailing one (Delete, Flag), right to left: do drags of 50, 55, 57 and 60 % of the row reveal or delete?")
    }

    @MainActor
    private func swipeProbe(_ name: String, _ sets: [SwipeSet], allowance: TimeInterval, _ question: String) async throws {
        executionTimeAllowance = allowance
        begin(name, question)
        let app = launchPlayground()
        let runner = XCUITestPhoneRunner()
        var reached = 0
        for (i, item) in sets.enumerated() {
            var opened = false
            if i == 0 {
                opened = await openProbe3(app, item.id)
            } else {
                opened = await openSibling(app, item.id)
            }
            guard opened else {
                fact("P13 \(item.set): could not produce (the screen \(item.id) was not reached)")
                continue
            }
            reached += 1
            if let skipped = await probeSwipeSet(app, item, runner: runner) { note("P13 \(item.set): \(skipped)") }
        }
        await finish(terminating: [])
        if reached == 0 { throw XCTSkip("no Swipe rows screen was reached") }
    }

    /// From one probe screen to another: Back to Fact probes 3, then the row `id`.
    @MainActor
    private func openSibling(_ app: XCUIApplication, _ id: String) async -> Bool {
        let back = app.navigationBars.buttons.element(boundBy: 0)
        tolerant("go back to Fact probes 3") { if back.exists && back.isHittable { back.tap() } }
        await pause(1)
        let row = app.buttons[id].firstMatch
        guard waitFor(row, "the \(id) row", 5) else { return false }
        tolerant("open \(id)") { if row.isHittable { row.tap() } }
        await pause(1)
        return true
    }

    /// The frame of the Cell holding the node labelled exactly `text` (else the node's own),
    /// from a snapshot.
    static func rowFrame(in tree: RawElement, labelled text: String) -> Rect? {
        guard let hit = hits(tree, { $0.label == text }).first else { return nil }
        if let cell = hit.ancestors.last(where: { $0.elementType == XCUIElementTypeCode.cell }) { return cell.frame }
        return hit.element.frame
    }

    /// Each attempt on a fresh row ("<prefix> N", N = 1, 2, …), with no element query in the
    /// loop (run 149: Jarvis's queries took 9-19 s late in P13b): one snapshot gives the row's
    /// frame, the drag is by coordinate (or the runner's), one snapshot after it says whether
    /// the row's own text is gone (deleted), which actions are on screen in the row's band
    /// (revealed), else nothing. A revealed row is removed with its first action (tapped by
    /// coordinate) so the next rows stay high. One fact per drag.
    @MainActor
    private func probeSwipeSet(_ app: XCUIApplication, _ item: SwipeSet, runner: XCUITestPhoneRunner) async -> String? {
        let status = app.staticTexts[item.statusId].firstMatch
        guard waitFor(status, "the \(item.set) rows' status line", 5) else { return "the \(item.set) screen has no status line" }
        let slug = item.prefix.lowercased().replacingOccurrences(of: " ", with: "-")
        guard let first = capture(app, Self.appBundleId, "rows-\(slug)", "The \(item.set) swipe rows before any swipe: each row's Cell and text, frames") else {
            return "no snapshot of the \(item.set) rows"
        }
        let screen = Size(width: first.frame.width, height: first.frame.height)
        guard screen.width > 0, screen.height > 0 else { return "no screen size in the snapshot" }
        note("\(item.set): the runner's swipe sends press(forDuration: 0.05, thenDragTo:) at XCTest's own speed (durationMs is not honoured); the XCUITest drags pass a velocity of length / duration")
        let remove = item.actions.first ?? "Delete"
        var row = 0
        var revealCaptured = false
        var deleteCaptured = false
        var deletedBy: [String] = []
        var revealedBy: [String] = []
        var before: RawElement? = first
        for attempt in item.attempts {
            row += 1
            let text = "\(item.prefix) \(row)"
            var tree = before
            if tree == nil { tree = snapshotTree(app, "Jarvis before the drag") }
            before = nil
            guard let tree, let frame = Self.rowFrame(in: tree, labelled: text) else {
                fact("P13 \(item.set), \(attempt.name): could not produce (\(text) not in the snapshot)")
                continue
            }
            let y = frame.y + frame.height / 2
            guard y > screen.height * 0.1, y < screen.height * 0.85 else {
                fact("P13 \(item.set), \(attempt.name): could not produce (\(text) at y \(Int(y)), outside the band a drag may use)")
                continue
            }
            var from = Point(x: 0, y: y)
            var to = Point(x: 0, y: y)
            var duration = attempt.ms
            var length: Double?
            if let share = attempt.screenShare { length = screen.width * share }
            if let share = attempt.rowShare { length = frame.width * share }
            if let points = attempt.points { length = points }
            if let length {
                let centre = frame.x + frame.width / 2
                let left = max(centre - length / 2, 1)
                let right = min(centre + length / 2, screen.width - 1)
                from = Point(x: attempt.rightward ? left : right, y: y)
                to = Point(x: attempt.rightward ? right : left, y: y)
            } else if let scroll = attempt.scroll {
                guard let path = Self.coreDrag("scroll", scroll, frame: frame, screen: screen) else { continue }
                from = path.from
                to = path.to
                duration = path.durationMs
            } else {
                guard let path = Self.coreSwipe(attempt.rightward ? "right" : "left", frame: frame, screen: screen) else { continue }
                from = path.from
                to = path.to
            }
            let travelled = abs(from.x - to.x)
            let ofRow = Int((travelled / max(frame.width, 1) * 100).rounded())
            let ofScreen = Int((travelled / screen.width * 100).rounded())
            if attempt.runner {
                await runnerDo(runner, .swipe(from: from, to: to, durationMs: duration), expecting: Self.appBundleId, "\(attempt.name) on \(text)")
            } else {
                let velocity = CGFloat(travelled / (Double(duration) / 1_000))
                let drag = from
                let drop = to
                tolerant("\(attempt.name) on \(text): XCUITest drag at velocity \(Int(velocity))") {
                    let origin = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
                    let start = origin.withOffset(CGVector(dx: drag.x, dy: drag.y))
                    let end = origin.withOffset(CGVector(dx: drop.x, dy: drop.y))
                    start.press(forDuration: 0.05, thenDragTo: end, withVelocity: XCUIGestureVelocity(rawValue: velocity), thenHoldForDuration: 0)
                }
            }
            await pause(1.1)
            let after = snapshotTree(app, "Jarvis after the drag")
            let deleted = after.map { tree in Self.hits(tree) { $0.label == text }.isEmpty } ?? false
            var revealed: [String] = []
            var removeFrame: Rect?
            if let after {
                for action in item.actions {
                    let inBand = Self.hits(after) { e in
                        e.elementType == XCUIElementTypeCode.button && e.label == action && e.frame.width > 0
                            && abs(e.frame.y + e.frame.height / 2 - y) < max(frame.height / 2, 10) && Self.onScreen(e.frame, after.frame)
                    }
                    if let hit = inBand.first {
                        revealed.append(action)
                        if action == remove { removeFrame = hit.element.frame }
                    }
                }
            }
            let statusText = after.flatMap { tree in Self.hits(tree) { $0.identifier == item.statusId }.first?.element.label } ?? "(no snapshot)"
            var outcome = after == nil ? "unknown (no snapshot after the drag)" : "nothing (the row in place, no action revealed)"
            if after != nil, deleted {
                outcome = "deleted"
                deletedBy.append("\(Int(travelled.rounded())) pt / \(ofRow) % of the row (\(attempt.name))")
            } else if !revealed.isEmpty {
                outcome = "revealed (\(revealed.joined(separator: ", ")))"
                revealedBy.append("\(Int(travelled.rounded())) pt / \(ofRow) % of the row")
            }
            let way = attempt.runner ? "runner, sent as \(duration) ms" : "XCUITest drag, \(duration) ms"
            fact("P13 \(item.set), \(attempt.name): \(Self.pointText(from)) -> \(Self.pointText(to)) on \(text) \(Self.rectText(frame)), \(Int(travelled.rounded())) pt = \(ofRow) % of the row (\(Int(frame.width.rounded())) pt) = \(ofScreen) % of the screen, \(way) -> \(outcome); status \"\(statusText)\"")
            if after != nil, deleted, !deleteCaptured {
                deleteCaptured = true
                capture(app, Self.appBundleId, "after-delete-\(row)", "After \(attempt.name) deleted \(text): the rows and the status line")
            }
            if !deleted, !revealed.isEmpty {
                if !revealCaptured {
                    revealCaptured = true
                    let shown = capture(app, Self.appBundleId, "revealed-\(row)", "After \(attempt.name) on \(text): the revealed actions (role, label, identifier, frame)")
                    report(shown, "revealed actions (nodes labelled \(item.actions.joined(separator: " or ")))") { item.actions.contains($0.label) }
                }
                let target = removeFrame ?? after.flatMap { tree in Self.hits(tree) { $0.identifier == item.statusId }.first?.element.frame }
                if let target {
                    tolerant("remove \(text) with its revealed \(remove), or close the actions (clean-up, by coordinate)") {
                        app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
                            .withOffset(CGVector(dx: target.x + target.width / 2, dy: target.y + target.height / 2)).tap()
                    }
                    await pause(0.8)
                }
            }
        }
        let deletedText = deletedBy.isEmpty ? "none" : deletedBy.joined(separator: "; ")
        let revealedText = revealedBy.isEmpty ? "none" : revealedBy.joined(separator: "; ")
        fact("P13 \(item.set) summary: deleted by \(deletedText); revealed by \(revealedText)")
        return nil
    }

    // MARK: - P13e, P13f, P13g: a row whose actions are revealed

    /// The Swipe rows, revealed screen (`Probe3SwipeRevealedFixture`) and its status line.
    private static let revealedScreenId = "probe3.swipesRevealed"
    private static let revealedStatusId = "probe3.swiperevealed.status"
    /// The second drags of P13e, as shares of the row's width (0.45 is Jarvis's bound).
    private static let secondShares = [0.10, 0.20, 0.30, 0.40, 0.45]

    /// What is done to a row after a first drag of 0.30 of its width (0.40 when 0.30 revealed
    /// nothing) revealed its actions.
    private enum SecondMove {
        /// A drag of `share` of the row, the way the first went, from the core's start on the
        /// row (where `coreSwipe` starts; the core shortens its drag from there).
        case drag(Double)
        /// The same, from 10 pt beside the revealed buttons, on the row's content still shown.
        case dragFromContent(Double)
        /// The same, from the centre of the revealed button that removes the row.
        case dragFromButton(Double)
        /// A drag of `share` the other way, from the core's start for that way.
        case opposite(Double)
        /// No XCUITest drag: the runner's swipe on the row twice, 1 s apart; `bounded`: the
        /// core's start shortened to 0.45 of the row (what Jarvis sends now), else P13a's
        /// unshortened core swipe.
        case runnerTwice(bounded: Bool)
        /// A tap in the middle of the row's content still shown.
        case tapContent
        /// A tap on the revealed button labelled so.
        case tapButton(String)
    }

    /// One row kind of the Swipe rows, revealed screen: its picker segment, its rows' text,
    /// its actions (the one that removes the row first), the way the finger reveals them, and
    /// the moves made (one fresh row each).
    private struct RevealSet {
        var segment: String
        var set: String
        var prefix: String
        var actions: [String]
        var rightward: Bool
        var moves: [SecondMove]
    }

    /// A row as one snapshot shows it: its text there or not (deleted), the text's and its
    /// Cell's frames, the action buttons in its band, the status line, an alert.
    private struct RowState {
        var tree: RawElement?
        var present = false
        var textFrame: Rect?
        var cellFrame: Rect?
        var buttons: [Probe3Case.Hit] = []
        var status = "(no snapshot)"
        var alert = false
    }

    private static let insetOneSet = RevealSet(
        segment: "Inset 1", set: "one trailing action, full width (400 pt rows)", prefix: "Inset one row", actions: ["Delete"], rightward: false,
        moves: secondShares.map { SecondMove.drag($0) }
            + [SecondMove.opposite(0.45), SecondMove.dragFromContent(0.45), SecondMove.dragFromButton(0.45),
               SecondMove.runnerTwice(bounded: false), SecondMove.runnerTwice(bounded: true)])
    private static let edgeOneSet = RevealSet(
        segment: "Edge 1", set: "one trailing action, edge to edge (440 pt rows)", prefix: "Edge one row", actions: ["Delete"], rightward: false,
        moves: secondShares.map { SecondMove.drag($0) } + [SecondMove.dragFromButton(0.45)])
    private static let insetTwoSet = RevealSet(
        segment: "Inset 2", set: "two trailing actions, full width (400 pt rows)", prefix: "Inset two row", actions: ["Delete", "Flag"], rightward: false,
        moves: secondShares.map { SecondMove.drag($0) }
            + [SecondMove.opposite(0.45), SecondMove.dragFromContent(0.45), SecondMove.dragFromButton(0.45), SecondMove.runnerTwice(bounded: true)])
    private static let leadingOneSet = RevealSet(
        segment: "Leading", set: "one leading action, left to right (440 pt rows)", prefix: "Leading one row", actions: ["Remove"], rightward: true,
        moves: secondShares.map { SecondMove.drag($0) } + [SecondMove.opposite(0.45), SecondMove.runnerTwice(bounded: true)])
    private static let tapSet = RevealSet(
        segment: "Taps", set: "button rows, two trailing actions, full width (400 pt rows)", prefix: "Tap row", actions: ["Delete", "Flag"], rightward: false,
        moves: [SecondMove.tapContent, SecondMove.tapButton("Delete"), SecondMove.tapButton("Flag")])

    @MainActor
    func testP13e_SecondSwipeOnRevealedRow() async throws {
        try await revealProbe("p13e-second-swipe", "P13e", [Self.insetOneSet, Self.edgeOneSet, Self.insetTwoSet, Self.leadingOneSet], treeOnly: false, allowance: 420,
                              "After a first drag of 0.30 of the row revealed its actions (the row then rests offset by them): does a second drag of 0.10, 0.20, 0.30, 0.40 or 0.45 of the row, the same way, delete it (offsets adding up), on 400 pt rows with one trailing Delete, 440 pt edge-to-edge rows with one, 400 pt rows with Delete and Flag, and 440 pt rows with one leading Remove? What do a second drag the other way, one from the revealed Delete and one from the content still shown do, and the runner's own swipe made twice in a row?")
    }

    @MainActor
    func testP13f_TreeWhileRevealed() async throws {
        let sets = [Self.insetOneSet, Self.edgeOneSet, Self.insetTwoSet, Self.leadingOneSet]
        try await revealProbe("p13f-revealed-tree", "P13f", sets, treeOnly: true, allowance: 180,
                              "With one row's actions revealed: are the action buttons in the tree, where (inside the row's Cell, siblings, another container), with which types, labels, identifiers and frames; does the Cell's frame move or shrink; does anything on the Cell (label, value, identifier, selected) say it is swiped; what does the runner's typed tree (the core's view) show?")
    }

    @MainActor
    func testP13g_TapOnRevealedRow() async throws {
        try await revealProbe("p13g-revealed-tap", "P13g", [Self.tapSet], treeOnly: false, allowance: 150,
                              "With a button row's actions (Delete, Flag) revealed: does a tap on the row's content close them or activate the row; does a tap on the revealed Delete delete at once or ask; what does a tap on the revealed Flag do?")
    }

    @MainActor
    private func revealProbe(_ name: String, _ tag: String, _ sets: [RevealSet], treeOnly: Bool, allowance: TimeInterval, _ question: String) async throws {
        executionTimeAllowance = allowance
        begin(name, question)
        let app = launchPlayground()
        let runner = XCUITestPhoneRunner()
        guard await openRevealedScreen(app) else {
            fact("\(tag): could not produce (the screen \(Self.revealedScreenId) was not reached)")
            await finish(terminating: [])
            throw XCTSkip("the Swipe rows, revealed screen was not reached")
        }
        note("\(tag): the XCUITest drags pass a velocity of length / 200 ms; the runner's swipe sends press(forDuration: 0.05, thenDragTo:) at XCTest's own speed")
        var reached = 0
        for item in sets {
            guard await selectKind(app, item) else {
                fact("\(tag) \(item.set): could not produce (the picker segment \"\(item.segment)\" was not found)")
                continue
            }
            reached += 1
            if treeOnly {
                await revealedTree(app, item, runner: runner)
            } else {
                await probeRevealSet(app, tag, item, runner: runner)
            }
        }
        await finish(terminating: [])
        if reached == 0 { throw XCTSkip("no row kind of the Swipe rows, revealed screen was reached") }
    }

    /// Opens Fact probes 3, then its last row, Swipe rows, revealed (scrolled to first when it
    /// is not hittable: the index may be longer than the screen).
    @MainActor
    private func openRevealedScreen(_ app: XCUIApplication) async -> Bool {
        let entry = app.buttons["playground.probes3"].firstMatch
        guard waitFor(entry, "the Playground's Fact probes 3 row", 10) else { return false }
        tolerant("open Fact probes 3") { entry.tap() }
        let top = app.buttons["probe3.symbols"].firstMatch
        guard waitFor(top, "the Fact probes 3 list", 5) else { return false }
        let row = app.buttons[Self.revealedScreenId].firstMatch
        if !hittableNow(row) {
            drag(app, from: 0.8, to: 0.4, "scroll Fact probes 3 to its last rows")
            await pause(1)
        }
        guard waitFor(row, "the \(Self.revealedScreenId) row", 5) else { return false }
        guard tapFirstButton(app, [Self.revealedScreenId], "open \(Self.revealedScreenId)") != nil else { return false }
        await pause(1)
        let status = app.staticTexts[Self.revealedStatusId].firstMatch
        return waitFor(status, "the revealed rows' status line", 5)
    }

    @MainActor
    private func selectKind(_ app: XCUIApplication, _ item: RevealSet) async -> Bool {
        let tapped = tapFirstButton(app, [item.segment], "choose the \(item.segment) rows")
        await pause(1)
        return tapped != nil
    }

    /// One snapshot of the row `text` (centred at `y`, `height` tall).
    @MainActor
    private func readRow(_ app: XCUIApplication, _ item: RevealSet, _ text: String, y: Double, height: Double, _ what: String) -> RowState {
        var state = RowState()
        guard let tree = snapshotTree(app, what) else { return state }
        state.tree = tree
        state.textFrame = Self.hits(tree) { $0.label == text }.first?.element.frame
        state.present = state.textFrame != nil
        state.cellFrame = Self.rowFrame(in: tree, labelled: text)
        state.buttons = Self.revealedButtons(tree, item.actions, y: y, height: height)
        state.status = Self.statusLabel(tree)
        state.alert = !Self.hits(tree) { $0.elementType == XCUIElementTypeCode.alert }.isEmpty
        return state
    }

    /// An XCUITest drag along `path` at velocity length / `ms` (P13a's drag).
    @MainActor
    private func xcDrag(_ app: XCUIApplication, _ path: (from: Point, to: Point), ms: UInt64 = 200, _ what: String) {
        let travelled = max(abs(path.from.x - path.to.x), 1)
        let velocity = CGFloat(travelled / (Double(ms) / 1_000))
        let drag = path.from
        let drop = path.to
        tolerant("\(what): XCUITest drag at velocity \(Int(velocity))") {
            let origin = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            let start = origin.withOffset(CGVector(dx: drag.x, dy: drag.y))
            let end = origin.withOffset(CGVector(dx: drop.x, dy: drop.y))
            start.press(forDuration: 0.05, thenDragTo: end, withVelocity: XCUIGestureVelocity(rawValue: velocity), thenHoldForDuration: 0)
        }
    }

    /// A tap at a point of `app` by coordinate.
    @MainActor
    private func tapPoint(_ app: XCUIApplication, _ x: Double, _ y: Double, _ what: String) {
        tolerant(what) {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0)).withOffset(CGVector(dx: x, dy: y)).tap()
        }
    }

    /// The first drag (0.30 of the row, from the core's start; a 0.40 one after it when 0.30
    /// revealed nothing and left the row), then 1 s, then one snapshot.
    @MainActor
    private func revealFirst(_ app: XCUIApplication, _ item: RevealSet, _ text: String, frame: Rect, y: Double, screen: Size, rest: RowState) async -> (state: RowState, text: String) {
        guard let start = Self.coreStart(frame, rightward: item.rightward, screen: screen) else {
            return (RowState(), "not made (no core start on the row)")
        }
        var said: [String] = []
        var state = RowState()
        for share in [0.30, 0.40] {
            let path = Self.dragPath(from: start, length: frame.width * share, rightward: item.rightward, screen: screen)
            xcDrag(app, path, "first drag of \(Self.percent(share)) of the row on \(text)")
            await pause(1)
            state = readRow(app, item, text, y: y, height: frame.height, "Jarvis 1 s after the first drag")
            said.append("drag \(Self.percent(share)) of the row \(Self.pathText(path, width: frame.width, [])) -> 1 s later \(Self.rowStateText(rest, state, text))")
            if state.tree == nil || !state.present || !state.buttons.isEmpty { break }
        }
        return (state, said.joined(separator: "; then "))
    }

    /// Removes the row `text` when it is still there: with its revealed removing action, else
    /// after a 0.40 drag reveals it (by coordinate). One try; a row left in place is noted.
    @MainActor
    private func removeRow(_ app: XCUIApplication, _ item: RevealSet, _ text: String, _ state: RowState, y: Double, height: Double, screen: Size) async {
        guard state.present else { return }
        let remove = item.actions.first ?? "Delete"
        var current = state
        if !current.buttons.contains(where: { $0.element.label == remove }) {
            if let frame = current.cellFrame, let start = Self.coreStart(frame, rightward: item.rightward, screen: screen) {
                let path = Self.dragPath(from: start, length: frame.width * 0.4, rightward: item.rightward, screen: screen)
                xcDrag(app, path, "clean-up: reveal \(text)'s actions (0.40 of the row)")
                await pause(1)
                current = readRow(app, item, text, y: y, height: height, "Jarvis at the clean-up")
                if !current.present { return }
            }
        }
        guard let button = current.buttons.first(where: { $0.element.label == remove }) else {
            note("clean-up: \(text) is still there and its \(remove) is not shown; left in place")
            return
        }
        let f = button.element.frame
        tapPoint(app, f.x + f.width / 2, f.y + f.height / 2, "clean-up: remove \(text) with its revealed \(remove) (by coordinate)")
        await pause(0.8)
    }

    /// P13e and P13g: each move on a fresh row ("<prefix> N", N = 1, 2, …). The first drag
    /// (or the runner's first swipe) and 1 s; one snapshot (the rest offset, the revealed
    /// buttons); the second move; 1.1 s; one snapshot (deleted, still revealed, closed; the
    /// status line says whether an action fired); the row is then removed. One fact per row.
    @MainActor
    private func probeRevealSet(_ app: XCUIApplication, _ tag: String, _ item: RevealSet, runner: XCUITestPhoneRunner) async {
        let slug = Self.slug(item.prefix)
        guard let rows = capture(app, Self.appBundleId, "rows-\(slug)", "The \(item.set) rows before any swipe: each row's Cell and text, frames") else {
            fact("\(tag) \(item.set): could not produce (no snapshot of the rows)")
            return
        }
        let screen = Size(width: rows.frame.width, height: rows.frame.height)
        guard screen.width > 0, screen.height > 0 else {
            fact("\(tag) \(item.set): could not produce (no screen size in the snapshot)")
            return
        }
        var row = 0
        var before: RawElement? = rows
        var revealCaptured = false
        var deleteCaptured = false
        var byOutcome: [String: [String]] = [:]
        for move in item.moves {
            row += 1
            let text = "\(item.prefix) \(row)"
            let name = Self.moveName(move)
            var tree = before
            if tree == nil { tree = snapshotTree(app, "Jarvis before the first drag") }
            before = nil
            guard let tree, let frame = Self.rowFrame(in: tree, labelled: text) else {
                fact("\(tag) \(item.set), \(name): could not produce (\(text) not in the snapshot)")
                continue
            }
            let y = frame.y + frame.height / 2
            guard y > screen.height * 0.1, y < screen.height * 0.85 else {
                fact("\(tag) \(item.set), \(name): could not produce (\(text) at y \(Int(y)), outside the band a drag may use)")
                continue
            }
            var rest = RowState()
            rest.tree = tree
            rest.present = true
            rest.textFrame = Self.hits(tree) { $0.label == text }.first?.element.frame
            rest.cellFrame = frame
            rest.status = Self.statusLabel(tree)
            let head = "\(tag) \(item.set), \(name): \(text) at rest \(Self.rectText(frame))"

            // The first drag.
            var mid = RowState()
            var firstText = ""
            var isRunner = false
            if case .runnerTwice(let bounded) = move {
                isRunner = true
                guard let path = Self.runnerPath(frame, bounded: bounded, rightward: item.rightward, screen: screen) else {
                    fact("\(head): could not produce (no core path on the row)")
                    continue
                }
                await runnerDo(runner, .swipe(from: path.from, to: path.to, durationMs: 200), expecting: Self.appBundleId, "\(name), first, on \(text)")
                await pause(1)
                mid = readRow(app, item, text, y: y, height: frame.height, "Jarvis 1 s after the runner's first swipe")
                firstText = "the runner's swipe \(Self.pathText(path, width: frame.width, [])) -> 1 s later \(Self.rowStateText(rest, mid, text))"
            } else {
                let first = await revealFirst(app, item, text, frame: frame, y: y, screen: screen, rest: rest)
                mid = first.state
                firstText = first.text
            }
            guard mid.tree != nil else {
                fact("\(head); first \(firstText): could not produce (no snapshot after the first drag)")
                continue
            }
            guard mid.present else {
                fact("\(head); first \(firstText): the first drag deleted the row; no second move")
                byOutcome["deleted by the first drag", default: []].append(Self.moveShort(move))
                continue
            }
            if mid.buttons.isEmpty, !isRunner {
                fact("\(head); first \(firstText): no action revealed; no second move")
                byOutcome["not revealed by the first drag", default: []].append(Self.moveShort(move))
                await removeRow(app, item, text, mid, y: y, height: frame.height, screen: screen)
                continue
            }
            if !revealCaptured, !mid.buttons.isEmpty {
                revealCaptured = true
                capture(app, Self.appBundleId, "revealed-\(slug)-\(row)", "After the first drag on \(text): the revealed actions (type, label, identifier, frame), the row's Cell and text frames (the rest offset)")
            }

            // The second move.
            let width = frame.width
            let cell = mid.cellFrame ?? frame
            let remove = item.actions.first ?? "Delete"
            var secondText = ""
            var missing = ""
            switch move {
            case .drag(let share):
                if let start = Self.coreStart(cell, rightward: item.rightward, screen: screen) {
                    let path = Self.dragPath(from: start, length: width * share, rightward: item.rightward, screen: screen)
                    xcDrag(app, path, "\(name) on \(text)")
                    secondText = Self.pathText(path, width: width, mid.buttons)
                } else {
                    missing = "no core start on the row's Cell"
                }
            case .dragFromContent(let share):
                let edge = Self.contentEdge(cell, mid.buttons, rightward: item.rightward)
                let start = Point(x: item.rightward ? edge + 10 : edge - 10, y: y)
                let path = Self.dragPath(from: start, length: width * share, rightward: item.rightward, screen: screen)
                xcDrag(app, path, "\(name) on \(text)")
                secondText = Self.pathText(path, width: width, mid.buttons)
            case .dragFromButton(let share):
                if let f = mid.buttons.first(where: { $0.element.label == remove })?.element.frame {
                    let start = Point(x: f.x + f.width / 2, y: f.y + f.height / 2)
                    let path = Self.dragPath(from: start, length: width * share, rightward: item.rightward, screen: screen)
                    xcDrag(app, path, "\(name) on \(text)")
                    secondText = Self.pathText(path, width: width, mid.buttons)
                } else {
                    missing = "no revealed \(remove)"
                }
            case .opposite(let share):
                if let start = Self.coreStart(cell, rightward: !item.rightward, screen: screen) {
                    let path = Self.dragPath(from: start, length: width * share, rightward: !item.rightward, screen: screen)
                    xcDrag(app, path, "\(name) on \(text)")
                    secondText = Self.pathText(path, width: width, mid.buttons)
                } else {
                    missing = "no core start on the row's Cell"
                }
            case .runnerTwice(let bounded):
                if let path = Self.runnerPath(cell, bounded: bounded, rightward: item.rightward, screen: screen) {
                    await runnerDo(runner, .swipe(from: path.from, to: path.to, durationMs: 200), expecting: Self.appBundleId, "\(name), second, on \(text)")
                    secondText = "the runner's swipe again " + Self.pathText(path, width: width, mid.buttons)
                } else {
                    missing = "no core path on the row's Cell"
                }
            case .tapContent:
                let edge = Self.contentEdge(cell, mid.buttons, rightward: item.rightward)
                let x = item.rightward ? (edge + cell.x + cell.width) / 2 : (cell.x + edge) / 2
                tapPoint(app, x, y, "\(name) on \(text) (by coordinate)")
                secondText = "a tap at \(Self.pointText(Point(x: x, y: y)))"
            case .tapButton(let label):
                if let f = mid.buttons.first(where: { $0.element.label == label })?.element.frame {
                    let point = Point(x: f.x + f.width / 2, y: f.y + f.height / 2)
                    tapPoint(app, point.x, point.y, "\(name) on \(text) (by coordinate)")
                    secondText = "a tap at \(Self.pointText(point)) on the revealed \"\(label)\" \(Self.rectText(f))"
                } else {
                    missing = "no revealed \(label)"
                }
            }
            guard missing.isEmpty else {
                fact("\(head); first \(firstText); second: could not produce (\(missing))")
                await removeRow(app, item, text, mid, y: y, height: frame.height, screen: screen)
                continue
            }
            await pause(1.1)
            let after = readRow(app, item, text, y: y, height: frame.height, "Jarvis after the second move")
            let kind = Self.outcomeKind(after)
            byOutcome[kind, default: []].append(Self.moveShort(move))
            let fired = after.tree != nil && after.status != mid.status ? "an action fired (status \"\(after.status)\")" : "no action fired (status unchanged)"
            fact("\(head); first \(firstText); second \(secondText) -> \(kind), \(fired): \(Self.rowStateText(rest, after, text))")
            if kind == "deleted", !deleteCaptured {
                deleteCaptured = true
                capture(app, Self.appBundleId, "after-second-delete-\(row)", "After \(name) deleted \(text): the rows and the status line")
            } else if Self.isTap(move) {
                capture(app, Self.appBundleId, "after-tap-\(row)", "After \(name) on \(text): is the row there, its actions still shown, an alert up, the status line")
            }
            await removeRow(app, item, text, after, y: y, height: frame.height, screen: screen)
        }
        var parts: [String] = []
        for kind in ["deleted", "still revealed", "closed", "unknown", "deleted by the first drag", "not revealed by the first drag"] {
            if let names = byOutcome[kind] { parts.append("\(kind): \(names.joined(separator: ", "))") }
        }
        fact("\(tag) \(item.set) summary: \(parts.isEmpty ? "no move made" : parts.joined(separator: "; "))")
    }

    /// P13f: row 1 revealed by a first drag of 0.30 (0.40 when needed); one capture at rest
    /// and one while revealed, read for where the actions sit and what changed on the Cell;
    /// the runner's typed tree of the same moment; then the row is removed.
    @MainActor
    private func revealedTree(_ app: XCUIApplication, _ item: RevealSet, runner: XCUITestPhoneRunner) async {
        let slug = Self.slug(item.prefix)
        let text = "\(item.prefix) 1"
        guard let before = capture(app, Self.appBundleId, "rows-\(slug)", "The \(item.set) rows at rest, before \(text) is dragged: the Cells, their texts, frames, values") else {
            fact("P13f \(item.set): could not produce (no snapshot of the rows)")
            return
        }
        let screen = Size(width: before.frame.width, height: before.frame.height)
        guard screen.width > 0, screen.height > 0, let frame = Self.rowFrame(in: before, labelled: text) else {
            fact("P13f \(item.set): could not produce (no screen size, or \(text) not in the snapshot)")
            return
        }
        let y = frame.y + frame.height / 2
        var rest = RowState()
        rest.tree = before
        rest.present = true
        rest.textFrame = Self.hits(before) { $0.label == text }.first?.element.frame
        rest.cellFrame = frame
        rest.status = Self.statusLabel(before)
        let first = await revealFirst(app, item, text, frame: frame, y: y, screen: screen, rest: rest)
        fact("P13f \(item.set): \(text) at rest \(Self.rectText(frame)); first \(first.text)")
        guard first.state.present, !first.state.buttons.isEmpty else {
            fact("P13f \(item.set): could not produce (no action revealed on \(text))")
            await removeRow(app, item, text, first.state, y: y, height: frame.height, screen: screen)
            return
        }
        guard let shown = capture(app, Self.appBundleId, "revealed-\(slug)", "With \(text)'s actions revealed (\(item.actions.joined(separator: ", "))): where are the action buttons (inside the row's Cell, siblings, another container), their types, labels, identifiers, frames; does the Cell's frame move or shrink; does anything on the Cell say it is swiped?") else {
            fact("P13f \(item.set): could not produce (no capture while revealed)")
            await removeRow(app, item, text, first.state, y: y, height: frame.height, screen: screen)
            return
        }
        let rowHeight = frame.height
        let inBand: (RawElement) -> Bool = { e in
            e.frame.width > 0 && e.frame.height > 0 && abs(e.frame.y + e.frame.height / 2 - y) < max(rowHeight / 2, 10)
        }
        for action in item.actions {
            let all = Self.hits(shown) { $0.label == action }
            let described: [String] = all.map { Self.buttonText($0, rowText: text) }
            fact("P13f \(item.set): nodes labelled \"\(action)\" while revealed: \(all.count)\(described.isEmpty ? "" : ": " + described.joined(separator: "; "))")
        }
        let cellBefore = Self.cellOf(before, text)
        let cellAfter = Self.cellOf(shown, text)
        fact("P13f \(item.set): the row's Cell at rest: \(Self.cellText(cellBefore))")
        fact("P13f \(item.set): the row's Cell while revealed: \(Self.cellText(cellAfter))")
        let textAfter = Self.hits(shown) { $0.label == text }.first?.element.frame
        let frameBefore = cellBefore.map { Self.rectText($0.frame) } ?? "none"
        let frameAfter = cellAfter.map { Self.rectText($0.frame) } ?? "none"
        fact("P13f \(item.set): the row's text at rest \(rest.textFrame.map { Self.rectText($0) } ?? "none"), while revealed \(textAfter.map { Self.rectText($0) } ?? "none"); the Cell's frame \(frameBefore) -> \(frameAfter), changed: \(Self.yesNo(frameBefore != frameAfter))")
        fact("P13f \(item.set): what changed on the Cell itself (label, value, identifier, selected, enabled, title, children): \(Self.cellMarkers(cellBefore, cellAfter))")
        let keysBefore = Set(Self.hits(before, inBand).map { Self.nodeKey($0.element) })
        let added = Self.hits(shown, inBand).filter { !keysBefore.contains(Self.nodeKey($0.element)) }
        let addedText: [String] = added.prefix(12).map { Self.buttonText($0, rowText: text) }
        fact("P13f \(item.set): nodes in the row's band while revealed that were not there at rest (by type, label, identifier): \(added.count)\(addedText.isEmpty ? "" : ": " + addedText.joined(separator: "; "))")
        report(shown, "P13f \(item.set): every node in the row's band while revealed", limit: 20, inBand)
        let look = await runnerLook("P13f \(item.set), \(text)'s actions revealed", "revealed-\(slug)", runner: runner)
        if let root = look?.root {
            let cells = Self.findAll(root) { $0.role == .cell && Self.subtreeMentions($0, text) }
            let cellsText: [String] = cells.prefix(3).map { Self.describeNode($0) }
            for action in item.actions {
                let nodes = Self.findAll(root) { ($0.label ?? "") == action }
                let held = cells.contains { cell in !Self.findAll(cell) { ($0.label ?? "") == action }.isEmpty }
                let described: [String] = nodes.prefix(4).map { Self.describeNode($0) }
                fact("P13f \(item.set): the runner's typed tree (what the core reads): \"\(action)\" as \(described.isEmpty ? "nothing" : described.joined(separator: "; ")); inside a typed cell holding \(text): \(Self.yesNo(held))")
            }
            fact("P13f \(item.set): the runner's typed cells holding \(text): \(cellsText.isEmpty ? "none" : cellsText.joined(separator: "; "))")
        } else {
            fact("P13f \(item.set): the runner's typed tree: not read (the runner's snapshot failed)")
        }
        let now = readRow(app, item, text, y: y, height: frame.height, "Jarvis before the clean-up")
        await removeRow(app, item, text, now, y: y, height: frame.height, screen: screen)
    }

    private static func moveName(_ move: SecondMove) -> String {
        switch move {
        case .drag(let share):
            return "second drag of \(percent(share)) of the row, the same way, from the core's start"
        case .dragFromContent(let share):
            return "second drag of \(percent(share)) of the row, the same way, from the content beside the revealed buttons"
        case .dragFromButton(let share):
            return "second drag of \(percent(share)) of the row, the same way, from the revealed destructive button"
        case .opposite(let share):
            return "second drag of \(percent(share)) of the row the other way, from the core's start"
        case .runnerTwice(let bounded):
            return bounded ? "the runner's swipe shortened to 45 % of the row (as Jarvis sends it), twice" : "the runner's core swipe (P13a's), twice"
        case .tapContent:
            return "a tap on the row's content"
        case .tapButton(let label):
            return "a tap on the revealed \(label)"
        }
    }

    private static func moveShort(_ move: SecondMove) -> String {
        switch move {
        case .drag(let share):
            return percent(share)
        case .dragFromContent(let share):
            return "\(percent(share)) from the content"
        case .dragFromButton(let share):
            return "\(percent(share)) from the button"
        case .opposite(let share):
            return "\(percent(share)) the other way"
        case .runnerTwice(let bounded):
            return bounded ? "the runner's bounded swipe twice" : "the runner's core swipe twice"
        case .tapContent:
            return "tap on the content"
        case .tapButton(let label):
            return "tap on \(label)"
        }
    }

    private static func isTap(_ move: SecondMove) -> Bool {
        switch move {
        case .tapContent, .tapButton:
            return true
        default:
            return false
        }
    }

    private static func outcomeKind(_ after: RowState) -> String {
        guard after.tree != nil else { return "unknown" }
        if !after.present { return "deleted" }
        return after.buttons.isEmpty ? "closed" : "still revealed"
    }

    /// The row in `state` against its rest: deleted, or the actions shown (type, label,
    /// identifier, frame, inside its Cell), the text's and the Cell's frames, the status line.
    private static func rowStateText(_ rest: RowState, _ state: RowState, _ text: String) -> String {
        guard state.tree != nil else { return "unknown (no snapshot)" }
        guard state.present else { return "deleted (status \"\(state.status)\"\(state.alert ? ", an alert is up" : ""))" }
        var parts: [String] = []
        if state.buttons.isEmpty {
            parts.append("no action shown")
        } else {
            let shown: [String] = state.buttons.map { buttonText($0, rowText: text) }
            parts.append("revealed: " + shown.joined(separator: "; "))
        }
        if let a = rest.textFrame, let b = state.textFrame {
            parts.append("the row's text \(rectText(a)) -> \(rectText(b)) (moved \(Int((b.x - a.x).rounded())) pt)")
        }
        if let a = rest.cellFrame, let b = state.cellFrame {
            parts.append("its Cell \(rectText(a)) -> \(rectText(b))")
        }
        parts.append("status \"\(state.status)\"")
        if state.alert { parts.append("an alert is up") }
        return parts.joined(separator: ", ")
    }

    /// `Button "Delete" id - {…}, in the row's Cell yes, parents Table > Cell > Other`.
    static func buttonText(_ hit: Probe3Case.Hit, rowText text: String) -> String {
        let e = hit.element
        let inCell = hit.ancestors.contains { a in
            a.elementType == XCUIElementTypeCode.cell && !Self.hits(a) { $0.label == text }.isEmpty
        }
        let id = e.identifier.isEmpty ? "-" : clip(e.identifier)
        return "\(XCUIElementTypeCode.name(e.elementType)) \"\(clip(e.label))\" id \(id) \(rectText(e.frame)), in the row's Cell \(yesNo(inCell)), parents \(ancestry(hit, 4))"
    }

    /// Buttons labelled as one of `actions`, drawn on screen in the band of the row centred
    /// at `y` (P13a's reading of "revealed").
    static func revealedButtons(_ tree: RawElement, _ actions: [String], y: Double, height: Double) -> [Probe3Case.Hit] {
        Self.hits(tree) { e in
            e.elementType == XCUIElementTypeCode.button && actions.contains(e.label) && e.frame.width > 0
                && abs(e.frame.y + e.frame.height / 2 - y) < max(height / 2, 10) && Self.onScreen(e.frame, tree.frame)
        }
    }

    static func statusLabel(_ tree: RawElement) -> String {
        Self.hits(tree) { $0.identifier == Self.revealedStatusId }.first?.element.label ?? "(no status line)"
    }

    /// The Cell holding the node labelled exactly `text`.
    static func cellOf(_ tree: RawElement, _ text: String) -> RawElement? {
        guard let hit = Self.hits(tree, { $0.label == text }).first else { return nil }
        return hit.ancestors.last(where: { $0.elementType == XCUIElementTypeCode.cell })
    }

    static func cellText(_ cell: RawElement?) -> String {
        guard let cell else { return "no Cell" }
        let kids: [String] = cell.children.map { Self.brief($0) + " " + Self.rectText($0.frame) }
        return "\(brief(cell)) \(rectText(cell.frame)) title \"\(clip(cell.title))\", \(cell.children.count) children: \(kids.joined(separator: " | "))"
    }

    static func cellMarkers(_ a: RawElement?, _ b: RawElement?) -> String {
        guard let a, let b else { return "not compared (no Cell found at rest or while revealed)" }
        var changed: [String] = []
        if a.label != b.label { changed.append("label \"\(clip(a.label))\" -> \"\(clip(b.label))\"") }
        if a.value != b.value { changed.append("value \"\(a.value ?? "nil")\" -> \"\(b.value ?? "nil")\"") }
        if a.identifier != b.identifier { changed.append("identifier \"\(a.identifier)\" -> \"\(b.identifier)\"") }
        if a.isSelected != b.isSelected { changed.append("selected \(yesNo(a.isSelected)) -> \(yesNo(b.isSelected))") }
        if a.isEnabled != b.isEnabled { changed.append("enabled \(yesNo(a.isEnabled)) -> \(yesNo(b.isEnabled))") }
        if a.title != b.title { changed.append("title \"\(clip(a.title))\" -> \"\(clip(b.title))\"") }
        if a.children.count != b.children.count { changed.append("children \(a.children.count) -> \(b.children.count)") }
        return changed.isEmpty ? "nothing" : changed.joined(separator: "; ")
    }

    static func nodeKey(_ e: RawElement) -> String {
        "\(e.elementType)|\(e.label)|\(e.identifier)"
    }

    /// Where the row's content still shown ends: the near edge of the revealed buttons (the
    /// leftmost left edge for trailing actions, the rightmost right edge for leading ones),
    /// else the Cell's far edge.
    static func contentEdge(_ cell: Rect, _ buttons: [Probe3Case.Hit], rightward: Bool) -> Double {
        let frames: [Rect] = buttons.map { $0.element.frame }
        if rightward {
            let edge = frames.map { $0.x + $0.width }.max() ?? cell.x
            return min(max(edge, cell.x), cell.x + cell.width)
        }
        let edge = frames.map { $0.x }.min() ?? (cell.x + cell.width)
        return max(min(edge, cell.x + cell.width), cell.x)
    }

    /// Where the core's swipe on `frame` starts (`coreSwipe`'s first point).
    static func coreStart(_ frame: Rect, rightward: Bool, screen: Size) -> Point? {
        Self.coreSwipe(rightward ? "right" : "left", frame: frame, screen: screen)?.from
    }

    /// The core's swipe on `frame`; `bounded`: shortened from its start to 0.45 of the row
    /// (`planned_drag` with `ROW_SWIPE_SHARE`).
    static func runnerPath(_ frame: Rect, bounded: Bool, rightward: Bool, screen: Size) -> (from: Point, to: Point)? {
        guard let core = Self.coreSwipe(rightward ? "right" : "left", frame: frame, screen: screen) else { return nil }
        guard bounded else { return core }
        let length = min(frame.width * 0.45, abs(core.to.x - core.from.x))
        return Self.dragPath(from: core.from, length: length, rightward: rightward, screen: screen)
    }

    /// A drag of `length` from `start`, the finger moving right when `rightward`, kept 1 pt
    /// inside the screen.
    static func dragPath(from start: Point, length: Double, rightward: Bool, screen: Size) -> (from: Point, to: Point) {
        let x = min(max(start.x, 1), screen.width - 1)
        let end = rightward ? min(x + length, screen.width - 1) : max(x - length, 1)
        return (Point(x: x, y: start.y), Point(x: end, y: start.y))
    }

    /// `(340, 250) -> (160, 250), 180 pt = 45 % of the row, starting on the revealed "Delete"`.
    static func pathText(_ path: (from: Point, to: Point), width: Double, _ buttons: [Probe3Case.Hit]) -> String {
        let travelled = abs(path.from.x - path.to.x)
        let share = Int((travelled / max(width, 1) * 100).rounded())
        let start = path.from
        let on = buttons.first { b in
            let f = b.element.frame
            return start.x >= f.x && start.x <= f.x + f.width && start.y >= f.y && start.y <= f.y + f.height
        }
        let onText = on.map { "on the revealed \"\($0.element.label)\"" } ?? "not on a revealed button"
        return "\(pointText(path.from)) -> \(pointText(path.to)), \(Int(travelled.rounded())) pt = \(share) % of the row, starting \(onText)"
    }

    static func percent(_ share: Double) -> String {
        "\(Int((share * 100).rounded())) %"
    }

    static func slug(_ text: String) -> String {
        text.lowercased().replacingOccurrences(of: " ", with: "-")
    }

    // MARK: - P14: the runner under a hung front app

    /// The three runner calls P14 makes while Jarvis's main thread is blocked.
    private static let hungCalls = ["snapshot(includeScreenshot: false)", "frontBundleId()", "perform(.tap) on Count tap"]

    @MainActor
    func testP14_RunnerUnderHungApp() async throws {
        executionTimeAllowance = 200
        begin("p14-hung-app", "With Jarvis's main thread blocked for 20 s (the front app hung): how long do the runner's snapshot, frontBundleId() and a tap take, and what do they return? Does a look hang past the runner's 16 s budget?")
        let app = launchPlayground()
        var reason: String?
        if await openProbe3(app, "probe3.system") {
            reason = await probeHung(app)
        } else {
            capture(app, Self.appBundleId, "no-screen", "Why was the System screen not reached?")
            reason = "the System screen was not reached"
        }
        await finish(terminating: [])
        if let reason { throw XCTSkip(reason) }
    }

    /// Per call: a fresh hang (the Hang button, tapped through SpringBoard's coordinates so the
    /// tap does not wait for Jarvis; the app blocks 1 s later, for 20 s), the call 2 s into
    /// the hang, timed; then a wait (with no query on Jarvis) until the hang is over. A call
    /// that does not return within 40 s ends the probe (XCTest cannot be interrupted: the
    /// progress file written right before it records that it started).
    @MainActor
    private func probeHung(_ app: XCUIApplication) async -> String? {
        let hang = app.buttons["probe3.system.hang"].firstMatch
        let count = app.buttons["probe3.system.count"].firstMatch
        let taps = app.staticTexts["probe3.system.taps"].firstMatch
        let status = app.staticTexts["probe3.system.status"].firstMatch
        guard waitFor(hang, "the Hang button", 5), let hangFrame = queryFrame(hang), let countFrame = queryFrame(count) else {
            capture(app, Self.appBundleId, "no-hang-button", "Why are the Hang and Count tap buttons missing?")
            return "no Hang or Count tap button on the System screen"
        }
        let runner = XCUITestPhoneRunner()
        await runnerLook("the System screen, Jarvis not hung (the baseline timings)", "calm", runner: runner)
        fact("P14 baseline (not hung): frontBundleId() \(lastRunnerFrontMs) ms -> \(lastRunnerFront); snapshot \(lastRunnerSnapshotMs) ms")
        for call in Self.hungCalls {
            checkpoint("starting a 20 s hang for the runner's \(call)", force: true)
            let tapped = Date()
            tapThroughSpringBoard(hangFrame, "tap Hang 20 s")
            let hangStart = tapped.addingTimeInterval(1)
            let hangEnd = hangStart.addingTimeInterval(20)
            let lead = hangStart.addingTimeInterval(2).timeIntervalSinceNow
            if lead > 0 { await pause(lead) }
            let into = String(format: "%.1f", Date().timeIntervalSince(hangStart))
            checkpoint("the runner's \(call), \(into) s into the hang", force: true)
            let start = Date()
            var answer = ""
            if call.hasPrefix("snapshot") {
                runner.onObservation = { [weak self] record in self?.keepRunnerRecord(record) }
                pendingRunnerSlug = "hung-snapshot"
                pendingRunnerQuestion = "What does Jarvis's runner report with Jarvis's main thread blocked (a snapshot 2 s into a 20 s hang)?"
                do {
                    let seen = try await runner.snapshot(includeScreenshot: false)
                    let nodes = seen.root.map { Self.typedCount($0) } ?? 0
                    let notes = seen.notes.isEmpty ? "none" : seen.notes.joined(separator: " | ")
                    answer = "of \(seen.appBundleId), \(nodes) typed nodes; notes: \(notes)"
                } catch {
                    answer = "threw \(error)"
                }
                runner.onObservation = nil
            } else if call.hasPrefix("frontBundleId") {
                do {
                    let id = try await runner.frontBundleId()
                    answer = id ?? "nil"
                } catch {
                    answer = "threw \(error)"
                }
            } else {
                let result = await runnerDo(runner, .tap(x: Double(countFrame.midX), y: Double(countFrame.midY)), expecting: Self.appBundleId, "tap Count tap during the hang")
                answer = result.map { r in r.ok ? "ok" : "failed: \(r.error ?? "?")" } ?? "threw (see the step above)"
            }
            let took = Date().timeIntervalSince(start)
            let returnedBeforeEnd = Date() < hangEnd
            let tookText = String(format: "%.1f", took)
            fact("P14 \(call) with Jarvis hung (called \(into) s into a 20 s hang): returned after \(tookText) s (\(returnedBeforeEnd ? "before" : "after") the hang ended; past the runner's 16 s budget: \(Self.yesNo(took > 16))) -> \(answer)")
            let rest = hangEnd.addingTimeInterval(1.5).timeIntervalSinceNow
            if rest > 0 { await pause(rest) }
            if took > 40 {
                fact("P14: \(call) took over 40 s: the remaining calls are not made")
                break
            }
            note("after the hang: status \"\(labelNow(status))\", taps \"\(labelNow(taps))\"")
        }
        fact("P14: the Count tap line after the hangs (the tap sent during a hang is delivered when the hang ends if it reads Taps 1): \"\(labelNow(taps))\"")
        await runnerLook("the System screen after the hangs", "after", runner: runner)
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
///
/// A test that runs out of its execution time allowance is killed with no teardown (run 139,
/// P9 and P5b: nothing was written). So the steps so far are also written as they go:
/// `probe3-<probe>-progress-NN.txt` (every step and every fact so far, cumulative; the
/// highest NN is the latest), at most every 30 s before a capture or a runner call, and
/// always right before a step that may block (`checkpoint(force:)`). The final result and
/// facts files supersede them.
///
/// A system app is opened only through `bringForward`: its icon must be in SpringBoard's tree
/// and it must reach the foreground within 10 s (run 139: activating Notes, which the
/// iOS 26.5 simulator does not have, blocked inside `activate()` for two minutes); an app
/// that did not come forward is not queried again, not even by `finish`.
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
    /// What the last `runnerLook` read: `frontBundleId()` (its top layer) and how long that
    /// call and the snapshot took (ms).
    var lastRunnerFront = "?"
    var lastRunnerFrontMs = 0
    var lastRunnerSnapshotMs = 0
    /// Apps `bringForward` found no icon for, or that did not start: never queried again.
    var unavailableApps: Set<String> = []
    var pendingRunnerSlug = "runner"
    var pendingRunnerQuestion = ""
    private var checkpoints = 0
    private var lastCheckpoint = Date()
    private var checkpointedLines = 0
    /// `begin` ran, `finish` ran to its end, and the last step noted: a probe whose body
    /// ended between the two fails in teardown with that step (run 146: P9b ended silently
    /// after the photo prompt and was reported as passed).
    private var began = false
    private var finished = false
    private var lastStep = "(none)"
    /// Open while `tolerant` scopes may match issues; closed by the teardown guard.
    let expectationGate = ExpectationGate()

    override func setUpWithError() throws {
        try super.setUpWithError()
        // QATestCase stops at the first failure; a probe records and goes on.
        continueAfterFailure = true
    }

    override func tearDownWithError() throws {
        // Never leave the simulator rotated, even when a probe ended early (teardown runs on
        // the main thread; checked, not assumed).
        if Thread.isMainThread {
            MainActor.assumeIsolated {
                if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
                failIfEndedEarly()
            }
        } else if began && !finished {
            XCTFail("probe round 3 \(probe): the test body ended before finish() (teardown off the main thread: the record was not written)")
        }
        try super.tearDownWithError()
    }

    /// A probe that began and did not reach the end of `finish()` (an early return, a
    /// swallowed throw, or XCTest ending the test after an issue it had been told to expect)
    /// FAILS here, naming the last step reached, after writing its progress file and leaving
    /// the simulator on the Home Screen.
    @MainActor
    private func failIfEndedEarly() {
        guard began, !finished else { return }
        let last = lastStep
        note("TEARDOWN: the probe did not reach finish(); last step: \(last)")
        checkpoint("teardown: the probe ended before finish()", force: true)
        let own = XCUIApplication()
        tolerant("terminate Jarvis (teardown)") { if own.state != .notRunning { own.terminate() } }
        tolerant("press Home (teardown)") { XCUIDevice.shared.press(.home) }
        began = false
        // Outside any expectation scope that can still match (see `tolerant`).
        expectationGate.close()
        XCTFail("probe round 3 \(probe): the test body ended before finish() with no failure of its own (an early return, a swallowed throw, or XCTest stopping the test after an expected issue); last step reached: \(last)")
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
        checkpoints = 0
        lastCheckpoint = Date()
        checkpointedLines = 0
        unavailableApps = []
        began = true
        finished = false
        note("probe round 3, \(name), started (\(ProcessInfo.processInfo.operatingSystemVersionString))")
        note("QUESTION: \(question)")
    }

    /// Writes every step and fact so far as `probe3-<probe>-progress-NN.txt` (cumulative), so
    /// a test killed at its time allowance still leaves its record. Unforced: at most every
    /// 30 s and only with new steps; forced: now (before a step that may block).
    @MainActor
    func checkpoint(_ why: String, force: Bool = false) {
        if !force {
            guard Date().timeIntervalSince(lastCheckpoint) >= 30, lines.count > checkpointedLines else { return }
        }
        checkpoints += 1
        lastCheckpoint = Date()
        note("progress \(checkpoints) written (next: \(why))")
        checkpointedLines = lines.count
        let n = String(format: "%02d", checkpoints)
        let head = "PROBE ROUND 3 \(probe): PROGRESS \(n), written before: \(why). Every step so far, then the facts so far; the result and facts files written when the probe ends supersede this.\n"
        let text = head + lines.joined(separator: "\n") + "\n\nFACTS SO FAR\n" + factsBody()
        attach(XCTAttachment(string: text), "probe3-\(probe)-progress-\(n).txt")
    }

    @MainActor
    func factsBody() -> String {
        facts.isEmpty ? "(no fact could be read mechanically: see the result file)\n" : facts.map { "- " + $0 }.joined(separator: "\n") + "\n"
    }

    @MainActor
    func note(_ text: String) {
        let line = String(format: "t+%6.1f s  ", Date().timeIntervalSince(started)) + text
        lines.append(line)
        if !text.hasPrefix("progress ") { lastStep = line.count > 300 ? String(line.prefix(300)) + "…" : line }
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
        checkpoint("finishing (terminate Jarvis and \(ids), Home Screen)", force: true)
        if XCUIDevice.shared.orientation != .portrait { XCUIDevice.shared.orientation = .portrait }
        let own = XCUIApplication()
        if own.state != .notRunning {
            tolerant("terminate Jarvis") { own.terminate() }
        }
        let unavailable = unavailableApps
        let reachable = ids.filter { !unavailable.contains($0) }
        if reachable.count < ids.count { note("not terminated (never came forward, not queried again): \(ids.filter { unavailable.contains($0) })") }
        await SystemAppAgentTests.startOnHomeScreen(terminating: reachable)
        note("probe ended: Home Screen, portrait")
        let text = "PROBE ROUND 3 \(probe): every step in time order\n" + lines.joined(separator: "\n") + "\n"
        attach(XCTAttachment(string: text), "probe3-\(probe)-result.txt")
        let factText = "PROBE ROUND 3 \(probe): the questions answered mechanically (question: answer)\n" + factsBody()
        attach(XCTAttachment(string: factText), "probe3-\(probe)-facts.txt")
        print(text)
        print(factText)
        finished = true
    }

    /// Captures `app`'s tree and a screenshot as `probe3-<probe>-NN-<slug>-tree.txt` and
    /// `….png`, opened by `question`. Returns the raw copy, or nil (recorded) when the snapshot
    /// failed.
    @MainActor
    @discardableResult
    func capture(_ app: XCUIApplication, _ bundleId: String, _ slug: String, _ question: String) -> RawElement? {
        checkpoint("capture of \(bundleId) (\(slug))")
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
        checkpoint("the runner's look: \(moment)")
        let runner = given ?? XCUITestPhoneRunner()
        pendingRunnerSlug = slug
        pendingRunnerQuestion = "What does Jarvis's runner (XCUITestPhoneRunner, the code the agent runs) report: \(moment)? Compare with the raw captures next to it."
        lastRunnerRecord = nil
        runner.onObservation = { [weak self] record in self?.keepRunnerRecord(record) }
        var front = "?"
        let frontStart = Date()
        do {
            let id = try await runner.frontBundleId()
            front = id ?? "nil"
        } catch {
            front = "threw \(error)"
        }
        lastRunnerFront = front
        lastRunnerFrontMs = Int(Date().timeIntervalSince(frontStart) * 1_000)
        note("\(moment): the runner's frontBundleId() (its top layer) is \(front) (\(lastRunnerFrontMs) ms)")
        var observation: RunnerObservation?
        let snapshotStart = Date()
        do {
            let seen = try await runner.snapshot(includeScreenshot: false)
            observation = seen
            lastRunnerSnapshotMs = Int(Date().timeIntervalSince(snapshotStart) * 1_000)
            let count = seen.root.map { Self.typedCount($0) } ?? 0
            let size = "\(seen.screenSize.width.compactText)x\(seen.screenSize.height.compactText)"
            let keyboard = seen.keyboardVisible ? "visible" : "hidden"
            let notes = seen.notes.isEmpty ? "none" : seen.notes.joined(separator: " | ")
            note("\(moment): the runner's look (\(lastRunnerSnapshotMs) ms) is of \(seen.appBundleId), \(count) typed nodes, keyboard \(keyboard), orientation \(seen.orientation), screen \(size); notes: \(notes)")
        } catch {
            lastRunnerSnapshotMs = Int(Date().timeIntervalSince(snapshotStart) * 1_000)
            note("\(moment): the runner's snapshot threw after \(lastRunnerSnapshotMs) ms: \(error)")
        }
        runner.onObservation = nil
        return observation
    }

    @MainActor
    func keepRunnerRecord(_ record: RunnerObservationRecord) {
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
        checkpoint("runner command: \(what)")
        let expect = bundleId.map { CommandExpect(bundleId: $0) }
        let start = Date()
        do {
            let result = try await runner.perform(command, expect: expect)
            let ms = Int(Date().timeIntervalSince(start) * 1_000)
            let detail = result.ok ? "" : ", failure \(String(describing: result.failure)), \(result.error ?? "")"
            note("runner \(what): \(Transcript.describe(command)), expect \(bundleId ?? "none") -> ok \(result.ok)\(detail) (\(ms) ms)")
            return result
        } catch {
            let ms = Int(Date().timeIntervalSince(start) * 1_000)
            note("runner \(what): \(Transcript.describe(command)) threw after \(ms) ms: \(error)")
            return nil
        }
    }

    /// The core's drag for a `kind` ("swipe": `direction` is the finger's; "scroll":
    /// `direction` is where the content comes from, so the finger moves the other way) left
    /// or right on `frame`, with the core's duration (`SWIPE_DURATION_MS` 200,
    /// `SCROLL_DURATION_MS` 400; gestures.rs `gesture`, `finger_direction`).
    static func coreDrag(_ kind: String, _ direction: String, frame: Rect?, screen: Size) -> (from: Point, to: Point, durationMs: UInt64)? {
        var finger = direction
        if kind == "scroll" { finger = direction == "left" ? "right" : "left" }
        guard let path = coreSwipe(finger, frame: frame, screen: screen) else { return nil }
        let duration: UInt64 = kind == "scroll" ? 400 : 200
        return (path.from, path.to, duration)
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
    /// identifier names a sheet, popover, dimming view or dismiss region. The fixtures' own
    /// identifiers (`probe3.…`, e.g. the triggers `probe3.modals.sheet`) never count (run 139
    /// counted the Modals screen's trigger buttons as modal-like nodes).
    static func modalish(_ e: RawElement) -> Bool {
        if [XCUIElementTypeCode.alert, XCUIElementTypeCode.dialog, XCUIElementTypeCode.sheet, XCUIElementTypeCode.popover].contains(e.elementType) {
            return true
        }
        if e.identifier.hasPrefix("probe3.") { return false }
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
        // Run 139 compared Emoji's top edge with the Keyboard node's bottom edge: Emoji and
        // Dictate reach y 956 while the Keyboard node ends at 898, so compare bottom edges.
        let keyboardBottom = keyboard.map { $0.frame.y + $0.frame.height }
        for (name, extra) in [("Emoji", emoji), ("Dictate", dictate)] {
            guard let keyboard, let keyboardBottom, let extra else { continue }
            let bottom = extra.frame.y + extra.frame.height
            let below = bottom > keyboardBottom + 1
            let wholly = extra.frame.y >= keyboardBottom - 1
            fact("\(what): \(name) extends below the Keyboard node's bottom edge: \(Self.yesNo(below)) (its bottom y \(Int(bottom.rounded())), the Keyboard node's \(Int(keyboardBottom.rounded())); wholly below it: \(Self.yesNo(wholly)); inside the Keyboard node's frame \(Self.rectText(keyboard.frame)): \(Self.yesNo(!below && extra.frame.y >= keyboard.frame.y - 1)))")
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

    /// Whether SpringBoard's tree holds an icon labelled (or identified) `label`, on any Home
    /// Screen page or in a folder (the caller is on the Home Screen). Queries SpringBoard only.
    @MainActor
    func iconInstalled(_ label: String) -> Bool {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        var found = false
        tolerant("look for the \(label) icon in SpringBoard's tree") {
            found = board.icons.matching(NSPredicate(format: "label == %@ OR identifier == %@", label, label)).firstMatch.exists
        }
        note("the \"\(label)\" icon in SpringBoard's tree: \(Self.yesNo(found))")
        return found
    }

    /// Brings the system app `bundleId` forward only when it can come forward within a short
    /// bound: its icon `icon` must be in SpringBoard's tree (activating an app that is not
    /// installed blocked inside `activate()` for two minutes in run 139), then it is
    /// activated and must be in the foreground within `timeout` s. Returns the app, or nil
    /// with the reason recorded as a fact; an app with no icon, or not running afterwards, is
    /// never queried again (`unavailableApps`). The caller is on the Home Screen.
    @MainActor
    func bringForward(_ bundleId: String, icon: String, timeout: TimeInterval = 10, fresh: Bool = true) async -> XCUIApplication? {
        guard iconInstalled(icon) else {
            unavailableApps.insert(bundleId)
            fact("\(bundleId): no \"\(icon)\" icon in SpringBoard's tree (not installed on this simulator, or not on the Home Screen): not activated")
            return nil
        }
        let app = XCUIApplication(bundleIdentifier: bundleId)
        if fresh, isRunning(app) {
            tolerant("terminate \(bundleId) for a fresh start") { app.terminate() }
        }
        checkpoint("activating \(bundleId) (bounded at \(timeout.compactText) s)", force: true)
        let start = Date()
        tolerant("activate \(bundleId)") { app.activate() }
        let activateSeconds = Date().timeIntervalSince(start)
        var up = false
        tolerant("wait for \(bundleId) in the foreground") { up = app.wait(for: .runningForeground, timeout: timeout) }
        let seconds = String(format: "%.1f", Date().timeIntervalSince(start))
        note("\(bundleId): activate() returned after \(String(format: "%.1f", activateSeconds)) s; in the foreground: \(Self.yesNo(up)) (\(seconds) s after the request)")
        guard up else {
            if !isRunning(app) { unavailableApps.insert(bundleId) }
            fact("\(bundleId): did not reach the foreground within \(timeout.compactText) s of activate() (state \(stateText(app))): not driven")
            return nil
        }
        return app
    }

    /// A tap at the centre of `element`'s query frame, synthesized through SpringBoard's
    /// coordinates: XCUITest then waits for SpringBoard to idle, never for the app under the
    /// point (run 139: a tap on Jarvis's Read pasteboard button waited 60 s for Jarvis, whose
    /// main thread was blocked in the read). The frame is read first, while the app is idle.
    @MainActor
    @discardableResult
    func tapThroughSpringBoard(_ frame: CGRect, _ what: String) -> Bool {
        let board = XCUIApplication(bundleIdentifier: Self.springboardId)
        let start = Date()
        let ok = tolerant(what) {
            let origin = board.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            origin.withOffset(CGVector(dx: frame.midX, dy: frame.midY)).tap()
        }
        note("\(what): tapped (\(Int(frame.midX)), \(Int(frame.midY))) through SpringBoard's coordinates; returned after \(String(format: "%.1f", Date().timeIntervalSince(start))) s")
        return ok
    }

    /// A tap at the centre of `element`'s query frame by coordinate on `app` (a SwiftUI Menu
    /// did not open from `tap()` in run 139). False when the element has no frame.
    @MainActor
    @discardableResult
    func tapCentre(_ app: XCUIApplication, _ element: XCUIElement, _ what: String) -> Bool {
        guard let frame = queryFrame(element) else {
            note("\(what): no frame by query; nothing tapped")
            return false
        }
        tolerant(what) {
            let origin = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            origin.withOffset(CGVector(dx: frame.midX, dy: frame.midY)).tap()
        }
        note("\(what): tapped the frame centre (\(Int(frame.midX)), \(Int(frame.midY))) of \(Self.frameText(frame)) by coordinate")
        return true
    }

    /// Taps the first existing button among `labels` (identifier or label) in `app`. Returns
    /// the one tapped.
    @MainActor
    @discardableResult
    func tapFirstButton(_ app: XCUIApplication, _ labels: [String], _ what: String) -> String? {
        var tapped: String?
        tolerant(what) {
            // Only a hittable button gets XCUITest's tap (a tap on one that is not hittable ends
            // the test, run 149); one that exists but is not hittable is tapped at its frame
            // centre by coordinate when that lies on the screen.
            for label in labels where tapped == nil {
                let button = app.buttons[label].firstMatch
                guard button.exists else { continue }
                if button.isHittable {
                    button.tap()
                    tapped = label
                } else {
                    let f = button.frame
                    let screen = app.frame
                    if f.width > 0, f.height > 0, screen.contains(CGPoint(x: f.midX, y: f.midY)) {
                        app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0)).withOffset(CGVector(dx: f.midX, dy: f.midY)).tap()
                        tapped = label + " (by coordinate: not hittable)"
                    }
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

    /// The first alert of `owner`, read from ONE snapshot of `owner` as data: its label, frame,
    /// texts and buttons (label, id). Returns the button labels.
    ///
    /// Run 146 (and 141, 142): this read the photo prompt element by element
    /// (`allElementsBoundByIndex`); XCTest raised "Failed to get matching snapshot: No matches
    /// found for Element at index 2" (Automation type mismatch: computed Alert vs Sheet,
    /// `_UIAlertControllerPhoneTVMacView`) inside the expected-failure scope, and the test then
    /// ended silently at its next XCTest call, before `finish()`, reported as passed. Reading
    /// a snapshot resolves no element by index. The node may be an Alert or a Sheet.
    @MainActor
    @discardableResult
    func readAlert(_ owner: XCUIApplication, _ ownerId: String, _ what: String) -> [String] {
        guard let alert = alertNode(owner, ownerId) else {
            note("\(what): no Alert or Sheet node in \(ownerId)'s snapshot")
            return []
        }
        let buttons = Self.hits(alert) { $0.elementType == XCUIElementTypeCode.button && !$0.label.isEmpty }
        let rows: [String] = buttons.map { hit in "\"\(hit.element.label)\" id \(hit.element.identifier.isEmpty ? "-" : hit.element.identifier) \(Self.rectText(hit.element.frame))" }
        let texts: [String] = Self.hits(alert) { $0.elementType == XCUIElementTypeCode.staticText && !$0.label.isEmpty }.map { "\"\(Self.clip($0.element.label))\"" }
        let kind = XCUIElementTypeCode.name(alert.elementType)
        note("\(what): \(kind) in \(ownerId)'s snapshot: label \"\(Self.clip(alert.label))\", id \(alert.identifier.isEmpty ? "-" : alert.identifier), frame \(Self.rectText(alert.frame)); texts: \(texts.joined(separator: ", ")); buttons: \(rows.joined(separator: ", "))")
        return buttons.map { $0.element.label }
    }

    /// One snapshot of `app` as data, or nil (recorded).
    @MainActor
    func snapshotTree(_ app: XCUIApplication, _ what: String) -> RawElement? {
        var raw: RawElement?
        tolerant("snapshot of \(what)") { raw = SnapshotCopy.copy(try app.snapshot()) }
        return raw
    }

    /// The first Alert (else Sheet) node in one snapshot of `owner`, or nil.
    @MainActor
    func alertNode(_ owner: XCUIApplication, _ ownerId: String) -> RawElement? {
        var raw: RawElement?
        tolerant("snapshot \(ownerId) for its alert") { raw = SnapshotCopy.copy(try owner.snapshot()) }
        guard let raw else { return nil }
        if let alert = Self.hits(raw, { $0.elementType == XCUIElementTypeCode.alert }).first { return alert.element }
        return Self.hits(raw, { $0.elementType == XCUIElementTypeCode.sheet }).first?.element
    }

    /// The button of a permission prompt that refuses (Don't Allow, Ask App Not to Track, …).
    static func denyingLabel(_ labels: [String]) -> String? {
        for key in ["don", "not", "deny", "cancel", "close"] {
            if let hit = labels.first(where: { $0.lowercased().contains(key) }) { return hit }
        }
        return nil
    }

    /// Taps `label` in `owner`'s first alert and waits (≤ 4 s) for it to go. The button's frame
    /// comes from a snapshot and the tap is a coordinate tap on `owner` (no element of the
    /// alert is resolved by query: see `readAlert`); by query only when the snapshot has no
    /// such button.
    @MainActor
    func tapInAlert(_ owner: XCUIApplication, _ label: String, _ what: String) async {
        let frame = alertNode(owner, "the alert's owner").flatMap { alert in Self.hits(alert) { $0.elementType == XCUIElementTypeCode.button && $0.label == label }.first?.element.frame }
        if let frame {
            tolerant(what) {
                let origin = owner.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
                origin.withOffset(CGVector(dx: frame.x + frame.width / 2, dy: frame.y + frame.height / 2)).tap()
            }
            note("\(what): tapped \"\(label)\" at its snapshot frame \(Self.rectText(frame))")
        } else {
            tolerant(what) {
                let button = owner.alerts.firstMatch.buttons[label].firstMatch
                if button.exists { button.tap() } else { note("\(what): no button \"\(label)\" in the alert") }
            }
        }
        let deadline = Date().addingTimeInterval(4)
        while existsNow(owner.alerts.firstMatch), Date() < deadline { await pause(0.3) }
        note("\(what): alert still up afterwards (by query): \(Self.yesNo(existsNow(owner.alerts.firstMatch)))")
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
        guard iconInstalled("Safari") else {
            unavailableApps.insert(Self.safariId)
            fact("Safari: no Safari icon in SpringBoard's tree: not opened")
            return nil
        }
        note("opening \(url) in Safari")
        checkpoint("opening \(url) in Safari (bounded at 10 s)", force: true)
        tolerant("open the URL") { XCUIDevice.shared.system.open(target) }
        var up = false
        tolerant("wait for Safari in the foreground") { up = safari.wait(for: .runningForeground, timeout: 10) }
        guard up else {
            note("Safari did not reach the foreground in 10 s (state \(stateText(safari)))")
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
        // Run 149: an XCUITest action that cannot be performed (a tap on an element that is not
        // hittable) records its issue and then ends the test body by an exception, from
        // inside this block; the block never returns, its expectation scope is never closed,
        // and it went on matching every later issue, the teardown guard's included. The gate
        // is closed before the guard fails, so a leaked scope matches nothing any more.
        let gate = expectationGate
        options.issueMatcher = { issue in
            guard gate.open else { return false }
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
            if issues.contains(where: { $0.contains("No matches found for Element at index") || $0.contains("Automation type mismatch") }) {
                note("WARNING \(what): an element could not be resolved (run 146: XCTest then ended the test at its next call; the teardown check reports it)")
            }
        }
        return ok
    }
}

/// Whether `tolerant`'s expected-failure matchers may still accept issues (shared with the
/// matcher closures XCTest keeps).
final class ExpectationGate: Sendable {
    private let state = Mutex(true)

    var open: Bool { state.withLock { $0 } }

    func close() {
        state.withLock { $0 = false }
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
