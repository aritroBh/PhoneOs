import Foundation
import UIKit
import XCTest
import JarvisCore
import JarvisKit

/// Probe P13h, on the Playground's Swipe rows, UIKit screen (`Probe3UIKitSwipesFixture`,
/// `ProbeFixtures3UIKit.swift`): what a UIKit `UITableView` and a `UICollectionView` list look
/// like in the accessibility tree while a row's swipe actions are revealed. P13e-g measured
/// SwiftUI `List` + `.swipeActions` only (the row's Cell keeps its size and moves sideways,
/// the action Buttons sit beside it); the safety gate's "revealed" rule reads that shape.
/// Per list and per direction (trailing, then leading): the tree at rest, a first drag of 0.30
/// of the row, the tree while revealed (the raw snapshot, the query view and the runner's typed
/// tree), a second drag of 0.30 (then 0.45 when still revealed), a drag the other way on a
/// fresh row, and a tap on the content of a fresh revealed row.
/// The probe RECORDS: only a screen or a list it cannot find fails it.
final class TreeFactProbe3UIKitTests: Probe3Case {
    private static let screenId = "probe3.swipesUIKit"
    private static let statusId = "uikit.status"

    private struct ListSpec {
        var name: String
        var slug: String
        var prefix: String
        var idPrefix: String
    }

    private static let lists = [
        ListSpec(name: "UIKit table", slug: "table", prefix: "Table row", idPrefix: "uikit.table.row."),
        ListSpec(name: "UIKit collection", slug: "collection", prefix: "Collection row", idPrefix: "uikit.collection.row."),
    ]

    /// The row under test: its text and identifier, the actions it reveals, the way the finger
    /// moves to reveal them, and where it rests (its Cell's frame, the band it sits in).
    private struct RowCtx {
        var text: String
        var id: String
        var actions: [String]
        var rightward: Bool
        var frame: Rect
        var y: Double
        var height: Double
    }

    /// A row as one snapshot shows it.
    private struct RowSnap {
        var tree: RawElement?
        var present = false
        var textFrame: Rect?
        var cell: RawElement?
        var buttons: [Probe3Case.Hit] = []
        var status = "(no snapshot)"
        var alert = false
    }

    @MainActor
    func testP13h_UIKitTreeWhileRevealed() async throws {
        executionTimeAllowance = 360
        begin("p13h-uikit", "With a UIKit UITableView row, and a UICollectionView list row, revealed by a swipe (trailing Delete and Flag, leading Remove): where are the action views in the tree (inside the row's Cell, a sibling, another container), with which types, labels, identifiers and frames; does the Cell's frame move or shrink; does its child count change; what do a second drag of 0.30 (then 0.45), a drag the other way and a tap on the content do?")
        let app = launchPlayground()
        let runner = XCUITestPhoneRunner()
        guard await openScreen(app) else {
            fact("P13h: could not produce (the screen \(Self.screenId) was not reached)")
            capture(app, Self.appBundleId, "no-screen", "Why was the Swipe rows, UIKit screen not reached?")
            await finish(terminating: [])
            XCTFail("P13h: the \(Self.screenId) screen was not reached")
            return
        }
        var missing: [String] = []
        if let tree = snapshotTree(app, "Jarvis on the UIKit swipe screen") {
            for list in Self.lists where Self.hits(tree, Self.mentions("\(list.prefix) 1")).isEmpty {
                missing.append(list.name)
            }
        } else {
            missing = Self.lists.map { $0.name }
        }
        for list in Self.lists {
            if missing.contains(list.name) {
                fact("P13h \(list.name): could not produce (the list's first row was not in the snapshot)")
                continue
            }
            await probeDirection(app, list, rightward: false, firstRow: 1, runner: runner)
            await probeDirection(app, list, rightward: true, firstRow: 4, runner: runner)
        }
        await finish(terminating: [])
        if !missing.isEmpty {
            XCTFail("P13h: not found on the screen: \(missing.joined(separator: ", "))")
        }
    }

    /// Opens Fact probes 3, then the UIKit swipe screen (scrolled to first when its row is not
    /// hittable: the index may be longer than the screen).
    @MainActor
    private func openScreen(_ app: XCUIApplication) async -> Bool {
        let entry = app.buttons["playground.probes3"].firstMatch
        guard waitFor(entry, "the Playground's Fact probes 3 row", 10) else { return false }
        tolerant("open Fact probes 3") { entry.tap() }
        let top = app.buttons["probe3.symbols"].firstMatch
        guard waitFor(top, "the Fact probes 3 list", 5) else { return false }
        let row = app.buttons[Self.screenId].firstMatch
        if !hittableNow(row) {
            drag(app, from: 0.8, to: 0.4, "scroll Fact probes 3 to its last rows")
            await pause(1)
        }
        guard waitFor(row, "the \(Self.screenId) row", 5) else { return false }
        guard tapFirstButton(app, [Self.screenId], "open \(Self.screenId)") != nil else { return false }
        await pause(1)
        let status = app.staticTexts[Self.statusId].firstMatch
        return waitFor(status, "the UIKit status line", 5)
    }

    // MARK: - One list, one direction

    @MainActor
    private func probeDirection(_ app: XCUIApplication, _ list: ListSpec, rightward: Bool, firstRow: Int, runner: XCUITestPhoneRunner) async {
        let dir = rightward ? "leading" : "trailing"
        let tag = "P13h \(list.name), \(dir)"
        let slug = "\(list.slug)-\(dir)"
        let actions = rightward ? ["Remove"] : ["Delete", "Flag"]
        guard let before = capture(app, Self.appBundleId, "\(slug)-rest", "The \(list.name) rows at rest, before a \(dir) swipe: each row's Cell, text, frame, children") else {
            fact("\(tag): could not produce (no snapshot at rest)")
            return
        }
        let screen = Size(width: before.frame.width, height: before.frame.height)
        guard screen.width > 0, screen.height > 0 else {
            fact("\(tag): could not produce (no screen size in the snapshot)")
            return
        }
        await phaseSecond(app, list, tag, slug, actions, rightward: rightward, number: firstRow, screen: screen, tree: before, runner: runner)
        await phaseOpposite(app, list, tag, actions, rightward: rightward, number: firstRow + 1, screen: screen)
        await phaseTap(app, list, tag, slug, actions, rightward: rightward, number: firstRow + 2, screen: screen)
    }

    /// Row `number`: revealed by a first drag, read while revealed, then a second drag of 0.30
    /// (and a third of 0.45 when it is still revealed).
    @MainActor
    private func phaseSecond(_ app: XCUIApplication, _ list: ListSpec, _ tag: String, _ slug: String, _ actions: [String], rightward: Bool, number: Int, screen: Size, tree: RawElement, runner: XCUITestPhoneRunner) async {
        guard let row = prepare(app, list, number, actions, rightward: rightward, screen: screen, tree: tree, tag) else { return }
        let ctx = row.ctx
        let head = "\(tag), \(ctx.text)"
        let first = await reveal(app, ctx, rest: row.rest, screen: screen)
        fact("\(head): at rest \(Self.rectText(ctx.frame)); first \(first.text)")
        guard first.state.present, Self.isRevealed(row.rest, first.state) else {
            fact("\(head): could not produce (no action revealed by a first drag)")
            await settle(app, ctx, rest: row.rest, first.state, screen: screen)
            return
        }
        if let shown = capture(app, Self.appBundleId, "\(slug)-revealed", "With \(ctx.text)'s actions revealed (\(actions.joined(separator: ", "))): where are the action views (inside the row's Cell, siblings, another container), their types, labels, identifiers, frames; does the Cell's frame move or shrink; does its child count change?") {
            attachQuery(app, "\(slug)-revealed", "The same moment as XCUITest's query view (debugDescription): the revealed actions of \(ctx.text)")
            await describeRevealed(ctx, row.rest, shown, head, slug, runner: runner)
        } else {
            fact("\(head): could not produce (no capture while revealed)")
        }
        var state = first.state
        for share in [0.30, 0.45] {
            let cell = state.cell?.frame ?? ctx.frame
            guard let start = TreeFactProbe3Tests.coreStart(cell, rightward: rightward, screen: screen) else {
                fact("\(head): second drag not made (no core start on the row's Cell)")
                break
            }
            let path = TreeFactProbe3Tests.dragPath(from: start, length: ctx.frame.width * share, rightward: rightward, screen: screen)
            let travelled = TreeFactProbe3Tests.pathText(path, width: ctx.frame.width, state.buttons)
            let statusBefore = state.status
            xcDrag(app, path, "drag of \(TreeFactProbe3Tests.percent(share)) of the row on \(ctx.text) (revealed)")
            await pause(1.1)
            let after = read(app, ctx, "Jarvis after the drag of \(TreeFactProbe3Tests.percent(share))")
            let kind = Self.outcome(row.rest, after)
            let fired = after.tree != nil && after.status != statusBefore ? "status changed to \"\(after.status)\"" : "status unchanged (\"\(after.status)\")"
            fact("\(head): drag of \(TreeFactProbe3Tests.percent(share)) of the row, the same way, on the revealed row, \(travelled) -> \(kind), \(fired): \(Self.stateText(row.rest, after, ctx.text))")
            state = after
            if kind != "still revealed" { break }
        }
        await settle(app, ctx, rest: row.rest, state, screen: screen)
    }

    /// Row `number`: revealed, then a drag of 0.30 the other way.
    @MainActor
    private func phaseOpposite(_ app: XCUIApplication, _ list: ListSpec, _ tag: String, _ actions: [String], rightward: Bool, number: Int, screen: Size) async {
        guard let row = prepare(app, list, number, actions, rightward: rightward, screen: screen, tree: nil, tag) else { return }
        let ctx = row.ctx
        let head = "\(tag), \(ctx.text)"
        let first = await reveal(app, ctx, rest: row.rest, screen: screen)
        guard first.state.present, Self.isRevealed(row.rest, first.state) else {
            fact("\(head): could not produce (no action revealed by a first drag; first \(first.text))")
            await settle(app, ctx, rest: row.rest, first.state, screen: screen)
            return
        }
        let cell = first.state.cell?.frame ?? ctx.frame
        guard let start = TreeFactProbe3Tests.coreStart(cell, rightward: !rightward, screen: screen) else {
            fact("\(head): could not produce (no core start for the drag the other way)")
            await settle(app, ctx, rest: row.rest, first.state, screen: screen)
            return
        }
        let path = TreeFactProbe3Tests.dragPath(from: start, length: ctx.frame.width * 0.30, rightward: !rightward, screen: screen)
        let travelled = TreeFactProbe3Tests.pathText(path, width: ctx.frame.width, first.state.buttons)
        xcDrag(app, path, "drag of 30 % of the row the other way on \(ctx.text) (revealed)")
        await pause(1.1)
        let after = read(app, ctx, "Jarvis after the drag the other way")
        let kind = Self.outcome(row.rest, after)
        fact("\(head): drag of 30 % of the row the other way on the revealed row, \(travelled) -> \(kind) (closed: \(Self.yesNo(kind == "closed"))), status \"\(after.status)\": \(Self.stateText(row.rest, after, ctx.text))")
        await settle(app, ctx, rest: row.rest, after, screen: screen)
    }

    /// Row `number`: revealed, then a tap on the row's content still shown.
    @MainActor
    private func phaseTap(_ app: XCUIApplication, _ list: ListSpec, _ tag: String, _ slug: String, _ actions: [String], rightward: Bool, number: Int, screen: Size) async {
        guard let row = prepare(app, list, number, actions, rightward: rightward, screen: screen, tree: nil, tag) else { return }
        let ctx = row.ctx
        let head = "\(tag), \(ctx.text)"
        let first = await reveal(app, ctx, rest: row.rest, screen: screen)
        guard first.state.present, Self.isRevealed(row.rest, first.state) else {
            fact("\(head): could not produce (no action revealed by a first drag; first \(first.text))")
            await settle(app, ctx, rest: row.rest, first.state, screen: screen)
            return
        }
        let x = Self.contentX(ctx, first.state, screen: screen)
        let statusBefore = first.state.status
        tapPoint(app, x, ctx.y, "tap on \(ctx.text)'s content (revealed), by coordinate")
        await pause(1.1)
        let after = read(app, ctx, "Jarvis after the tap")
        let kind = Self.outcome(row.rest, after)
        capture(app, Self.appBundleId, "\(slug)-after-tap", "After a tap on \(ctx.text)'s content while its actions were revealed: is the row there, its actions still shown, anything opened, the status line")
        let selected = after.tree != nil && after.status != statusBefore
        let result: String
        switch kind {
        case "closed": result = "the actions closed"
        case "still revealed": result = "nothing visible: the actions are still shown"
        case "deleted": result = "the row was deleted"
        default: result = "unknown (no snapshot)"
        }
        fact("\(head): a tap at \(Self.pointText(Point(x: x, y: ctx.y))) on the content of the revealed row -> \(result); navigated: no screen in this fixture to navigate to, the status line says \"\(after.status)\" (changed by the tap: \(Self.yesNo(selected))), an alert is up: \(Self.yesNo(after.alert)): \(Self.stateText(row.rest, after, ctx.text))")
        await settle(app, ctx, rest: row.rest, after, screen: screen)
    }

    // MARK: - Reading and moving

    /// The row's text, identifier, Cell and frame from one snapshot (`tree`, or a new one).
    @MainActor
    private func prepare(_ app: XCUIApplication, _ list: ListSpec, _ number: Int, _ actions: [String], rightward: Bool, screen: Size, tree given: RawElement?, _ tag: String) -> (ctx: RowCtx, rest: RowSnap)? {
        let text = "\(list.prefix) \(number)"
        let id = "\(list.idPrefix)\(number)"
        let snapshot = given ?? snapshotTree(app, "Jarvis before the first drag on \(text)")
        guard let tree = snapshot else {
            fact("\(tag), \(text): could not produce (no snapshot before the first drag)")
            return nil
        }
        let cell = Self.cellOf(tree, text, id)
        let textFrame = Self.hits(tree) { $0.label == text && $0.elementType == XCUIElementTypeCode.staticText }.first?.element.frame
        guard let frame = cell?.frame ?? textFrame else {
            fact("\(tag), \(text): could not produce (\(text) not in the snapshot)")
            return nil
        }
        let y = frame.y + frame.height / 2
        guard y > screen.height * 0.1, y < screen.height * 0.85 else {
            fact("\(tag), \(text): could not produce (\(text) at y \(Int(y)), outside the band a drag may use)")
            return nil
        }
        var rest = RowSnap()
        rest.tree = tree
        rest.present = true
        rest.textFrame = textFrame
        rest.cell = cell
        rest.status = Self.statusLabel(tree)
        let ctx = RowCtx(text: text, id: id, actions: actions, rightward: rightward, frame: frame, y: y, height: frame.height)
        return (ctx, rest)
    }

    /// One snapshot of the row `ctx`.
    @MainActor
    private func read(_ app: XCUIApplication, _ ctx: RowCtx, _ what: String) -> RowSnap {
        var state = RowSnap()
        guard let tree = snapshotTree(app, what) else { return state }
        state.tree = tree
        state.present = !Self.hits(tree) { $0.label == ctx.text || $0.identifier == ctx.id }.isEmpty
        state.textFrame = Self.hits(tree) { $0.label == ctx.text && $0.elementType == XCUIElementTypeCode.staticText }.first?.element.frame
        state.cell = Self.cellOf(tree, ctx.text, ctx.id)
        state.buttons = Self.actionNodes(tree, ctx.actions, y: ctx.y, height: ctx.height)
        state.status = Self.statusLabel(tree)
        state.alert = !Self.hits(tree) { $0.elementType == XCUIElementTypeCode.alert }.isEmpty
        return state
    }

    /// The first drag (0.30 of the row, from the core's start; a 0.40 one after it when 0.30
    /// revealed nothing and left the row), then 1 s, then one snapshot.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ ctx: RowCtx, rest: RowSnap, screen: Size) async -> (state: RowSnap, text: String) {
        guard let start = TreeFactProbe3Tests.coreStart(ctx.frame, rightward: ctx.rightward, screen: screen) else {
            return (RowSnap(), "not made (no core start on the row)")
        }
        var said: [String] = []
        var state = RowSnap()
        for share in [0.30, 0.40] {
            let path = TreeFactProbe3Tests.dragPath(from: start, length: ctx.frame.width * share, rightward: ctx.rightward, screen: screen)
            xcDrag(app, path, "first drag of \(TreeFactProbe3Tests.percent(share)) of the row on \(ctx.text)")
            await pause(1)
            state = read(app, ctx, "Jarvis 1 s after the first drag")
            said.append("drag \(TreeFactProbe3Tests.percent(share)) of the row \(TreeFactProbe3Tests.pathText(path, width: ctx.frame.width, [])) -> 1 s later \(Self.stateText(rest, state, ctx.text))")
            if state.tree == nil || !state.present || Self.isRevealed(rest, state) { break }
        }
        return (state, said.joined(separator: "; then "))
    }

    /// Closes the row's actions when they are still shown: a tap on its content, else its
    /// first action (by coordinate). A row left in place is noted.
    @MainActor
    private func settle(_ app: XCUIApplication, _ ctx: RowCtx, rest: RowSnap, _ state: RowSnap, screen: Size) async {
        guard state.present, Self.isRevealed(rest, state) else { return }
        let x = Self.contentX(ctx, state, screen: screen)
        tapPoint(app, x, ctx.y, "clean-up: close \(ctx.text)'s actions with a tap on its content (by coordinate)")
        await pause(0.8)
        let current = read(app, ctx, "Jarvis after the clean-up tap")
        guard current.present, Self.isRevealed(rest, current) else { return }
        let remove = ctx.actions.first ?? "Delete"
        if let button = current.buttons.first(where: { $0.element.label == remove }) {
            let f = button.element.frame
            tapPoint(app, f.x + f.width / 2, f.y + f.height / 2, "clean-up: remove \(ctx.text) with its revealed \(remove) (by coordinate)")
            await pause(0.8)
        } else {
            note("clean-up: \(ctx.text) is still revealed and its \(remove) is not found; left in place")
        }
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

    // MARK: - What the tree says

    /// The revealed row: its first capture, then what is in the tree that was not at rest, the
    /// Cell's frame and children against the rest, and the runner's typed tree of the moment.
    @MainActor
    private func describeRevealed(_ ctx: RowCtx, _ rest: RowSnap, _ shown: RawElement, _ head: String, _ slug: String, runner: XCUITestPhoneRunner) async {
        guard let before = rest.tree else { return }
        let text = ctx.text
        let y = ctx.y
        let rowHeight = ctx.height
        for action in ctx.actions {
            let all = Self.hits(shown) { $0.label == action }
            let described: [String] = all.map { TreeFactProbe3Tests.buttonText($0, rowText: text) }
            fact("\(head): nodes labelled \"\(action)\" while revealed: \(all.count)\(described.isEmpty ? "" : ": " + described.joined(separator: "; "))")
        }
        let cellBefore = Self.cellOf(before, text, ctx.id)
        let cellAfter = Self.cellOf(shown, text, ctx.id)
        fact("\(head): the row's Cell at rest: \(TreeFactProbe3Tests.cellText(cellBefore))")
        fact("\(head): the row's Cell while revealed: \(TreeFactProbe3Tests.cellText(cellAfter))")
        let frameBefore = cellBefore.map { Self.rectText($0.frame) } ?? "none"
        let frameAfter = cellAfter.map { Self.rectText($0.frame) } ?? "none"
        fact("\(head): the Cell's frame \(frameBefore) -> \(frameAfter), changed: \(Self.yesNo(frameBefore != frameAfter))")
        if let p = cellBefore?.frame, let q = cellAfter?.frame {
            fact("\(head): the Cell moved by \(Int((q.x - p.x).rounded())) pt sideways, its width \(Int(p.width.rounded())) -> \(Int(q.width.rounded())) pt (shrunk: \(Self.yesNo(q.width < p.width - 2)))")
        }
        let childrenBefore = cellBefore?.children.count ?? -1
        let childrenAfter = cellAfter?.children.count ?? -1
        fact("\(head): the Cell's child count \(childrenBefore) -> \(childrenAfter), changed: \(Self.yesNo(childrenBefore != childrenAfter))")
        fact("\(head): what changed on the Cell itself (label, value, identifier, selected, enabled, title, children): \(TreeFactProbe3Tests.cellMarkers(cellBefore, cellAfter))")
        let inBand: (RawElement) -> Bool = { e in
            e.frame.width > 0 && e.frame.height > 0 && abs(e.frame.y + e.frame.height / 2 - y) < max(rowHeight / 2, 10)
        }
        let keysBefore = Set(Self.hits(before, inBand).map { TreeFactProbe3Tests.nodeKey($0.element) })
        let added = Self.hits(shown, inBand).filter { !keysBefore.contains(TreeFactProbe3Tests.nodeKey($0.element)) }
        let addedText: [String] = added.prefix(12).map { TreeFactProbe3Tests.buttonText($0, rowText: text) }
        fact("\(head): nodes in the row's band while revealed that were not there at rest (by type, label, identifier): \(added.count)\(addedText.isEmpty ? "" : ": " + addedText.joined(separator: "; "))")
        report(shown, "\(head): every node in the row's band while revealed", limit: 20, inBand)
        let look = await runnerLook("\(head), actions revealed", "\(slug)-revealed", runner: runner)
        if let root = look?.root {
            let cells = Self.findAll(root) { $0.role == .cell && Self.subtreeMentions($0, text) }
            let cellsText: [String] = cells.prefix(3).map { Self.describeNode($0) }
            for action in ctx.actions {
                let nodes = Self.findAll(root) { ($0.label ?? "") == action }
                let held = cells.contains { cell in !Self.findAll(cell) { ($0.label ?? "") == action }.isEmpty }
                let described: [String] = nodes.prefix(4).map { Self.describeNode($0) }
                fact("\(head): the runner's typed tree (what the core reads): \"\(action)\" as \(described.isEmpty ? "nothing" : described.joined(separator: "; ")); inside a typed cell holding \(text): \(Self.yesNo(held))")
            }
            fact("\(head): the runner's typed cells holding \(text): \(cellsText.isEmpty ? "none" : cellsText.joined(separator: "; "))")
        } else {
            fact("\(head): the runner's typed tree: not read (the runner's snapshot failed)")
        }
    }

    /// The Cell holding the row: a Cell node named by the row's text or identifier, else the
    /// nearest Cell above a node so named.
    private static func cellOf(_ tree: RawElement, _ text: String, _ id: String) -> RawElement? {
        let found = Self.hits(tree) { $0.label == text || $0.identifier == id }
        for hit in found where hit.element.elementType == XCUIElementTypeCode.cell {
            return hit.element
        }
        for hit in found {
            if let cell = hit.ancestors.last(where: { $0.elementType == XCUIElementTypeCode.cell }) { return cell }
        }
        return nil
    }

    /// Nodes labelled as one of `actions`, of any type, drawn on screen in the band of the row
    /// centred at `y` (what a row's revealed buttons would be, wherever UIKit puts them).
    private static func actionNodes(_ tree: RawElement, _ actions: [String], y: Double, height: Double) -> [Probe3Case.Hit] {
        Self.hits(tree) { e in
            actions.contains(e.label) && e.frame.width > 0 && e.frame.height > 0
                && abs(e.frame.y + e.frame.height / 2 - y) < max(height / 2, 10) && Self.onScreen(e.frame, tree.frame)
        }
    }

    private static func statusLabel(_ tree: RawElement) -> String {
        Self.hits(tree) { $0.identifier == Self.statusId }.first?.element.label ?? "(no status line)"
    }

    /// The row's Cell moved or changed width against its rest.
    private static func moved(_ rest: RowSnap, _ now: RowSnap) -> Bool {
        guard let p = rest.cell?.frame, let q = now.cell?.frame else { return false }
        return abs(p.x - q.x) > 2 || abs(p.width - q.width) > 2
    }

    /// Actions shown in the row's band, or the Cell moved.
    private static func isRevealed(_ rest: RowSnap, _ now: RowSnap) -> Bool {
        !now.buttons.isEmpty || moved(rest, now)
    }

    private static func outcome(_ rest: RowSnap, _ after: RowSnap) -> String {
        guard after.tree != nil else { return "unknown" }
        if !after.present { return "deleted" }
        return isRevealed(rest, after) ? "still revealed" : "closed"
    }

    /// An x on the row's content still shown (between the Cell's near edge and the revealed
    /// actions), kept inside the screen.
    private static func contentX(_ ctx: RowCtx, _ state: RowSnap, screen: Size) -> Double {
        let cell = state.cell?.frame ?? ctx.frame
        let edge = TreeFactProbe3Tests.contentEdge(cell, state.buttons, rightward: ctx.rightward)
        let x = ctx.rightward ? (edge + cell.x + cell.width) / 2 : (cell.x + edge) / 2
        return min(max(x, 10), screen.width - 10)
    }

    /// The row in `state` against its rest: deleted, or the actions shown, the text's and the
    /// Cell's frames, the status line.
    private static func stateText(_ rest: RowSnap, _ state: RowSnap, _ text: String) -> String {
        guard state.tree != nil else { return "unknown (no snapshot)" }
        guard state.present else { return "deleted (status \"\(state.status)\"\(state.alert ? ", an alert is up" : ""))" }
        var parts: [String] = []
        if state.buttons.isEmpty {
            parts.append("no action node in the row's band")
        } else {
            let shown: [String] = state.buttons.map { TreeFactProbe3Tests.buttonText($0, rowText: text) }
            parts.append("action nodes: " + shown.joined(separator: "; "))
        }
        if let a = rest.textFrame, let b = state.textFrame {
            parts.append("the row's text \(rectText(a)) -> \(rectText(b)) (moved \(Int((b.x - a.x).rounded())) pt)")
        }
        if let a = rest.cell?.frame, let b = state.cell?.frame {
            parts.append("its Cell \(rectText(a)) -> \(rectText(b))")
        }
        parts.append("status \"\(state.status)\"")
        if state.alert { parts.append("an alert is up") }
        return parts.joined(separator: ", ")
    }
}
