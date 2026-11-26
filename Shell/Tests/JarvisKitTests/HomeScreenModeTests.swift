import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// Run 146 (probe P15): the Today view, the App Library, an open folder and the Home
/// Screen's edit mode all hold the Home Screen's icon container (`Home screen icons`), so the
/// runner read each of them as the Home Screen. The edit mode, the App Library and an open
/// folder are now SpringBoard states reported as the unidentified layer, named
/// (`SystemSurface`); the Today view stays the Home Screen (the core shows its widgets as
/// stubs).
@Suite("Home Screen states: edit mode, App Library, open folder, Today view (SystemSurface)")
struct HomeScreenModeTests {
    let sb = TopLayer.springBoard

    private func expected(_ c: RealHomeScreenModeTrees.Capture) -> SystemSurface? {
        c.mode.flatMap { SystemSurface(rawValue: $0) }
    }

    @Test("each P15 capture: what SpringBoard's tree shows, and the top layer the runner reports")
    func p15Captures() throws {
        for c in RealHomeScreenModeTrees.all {
            let raw = try RawTreeText.parse(c.text)
            // Before: every one of them read as the Home Screen.
            #expect(FrontApp.homeScreenShowing(raw), "\(c.name)")
            let layers = SystemLayers.read(springBoard: raw)
            #expect(layers.surface == expected(c), "\(c.name)")
            #expect(!layers.alert, "\(c.name)")
            let top = TopLayer.decide(systemAlertUp: layers.alert, systemSurface: layers.surface, spotlightInFront: false,
                                      foreground: sb, springBoard: .homeScreen)
            if let mode = expected(c) {
                #expect(FrontApp.isUnidentified(top.bundleId), "\(c.name)")
                #expect(top.surface == mode)
                #expect(top.note == mode.note(over: nil))
                // A tap planned on the plain Home Screen is stale on this layer.
                #expect(FrontAppGuard.refusalNextToGesture(CommandExpect(bundleId: sb), front: top.bundleId)?.failure == .stale,
                        "\(c.name)")
            } else {
                #expect(top.bundleId == sb, "\(c.name)")
                #expect(top.note == nil)
            }
        }
    }

    @Test("the identifiers are in no other SpringBoard capture: the 45 older ones (runs 134 to 139) and P12a's")
    func identifiersAreUnique() throws {
        let modes: Set<SystemSurface> = [.homeEditMode, .appLibrary, .openFolder]
        var trees: [(String, String)] = RealSpringBoardTrees.all.map { ($0.name, $0.text) }
        trees += RealSystemSurfaceTrees.all.map { ($0.name, $0.text) }
        #expect(trees.count >= 45)
        for (name, text) in trees {
            let raw = try RawTreeText.parse(text)
            if let s = SystemSurface.up(inSpringBoard: raw) {
                #expect(!modes.contains(s), "\(name): \(s)")
            }
            var stack = [raw]
            while let e = stack.popLast() {
                for m in modes { #expect(!m.matches(e.identifier), "\(name): \(e.identifier)") }
                stack.append(contentsOf: e.children)
            }
        }
    }

    private func edit(_ e: inout RawElement, _ f: (inout RawElement) -> Void) {
        f(&e)
        for i in e.children.indices { edit(&e.children[i], f) }
    }

    /// An icon renamed: its label and identifier are the app's display name.
    private func forgedIcons(_ text: String) throws -> RawElement {
        var raw = try RawTreeText.parse(text)
        var k = 0
        edit(&raw) { e in
            guard e.elementType == XCUIElementTypeCode.icon, e.value == nil || e.value == "" else { return }
            if e.label == "Maps" || e.label == "Fitness" {
                let name = k == 0 ? "DeleteButton" : "Notes, Folder name"
                k += 1
                e.label = name
                e.identifier = name
            }
        }
        return raw
    }

    @Test("review pass 30 (L1): an app named like a marker, or a widget holding one, makes no Home Screen state")
    func markersOnlyFromSpringBoardsOwnNodes() throws {
        #expect(SystemSurface.up(inSpringBoard: try forgedIcons(RealHomeScreenModeTrees.home.text)) == nil)
        var widget = try RawTreeText.parse(RealHomeScreenModeTrees.home.text)
        edit(&widget) { e in
            guard (e.value ?? "").hasPrefix("Widget") else { return }
            var badge = RawElement(elementType: XCUIElementTypeCode.button, frame: e.frame)
            badge.identifier = "DeleteButton"
            var icon = RawElement(elementType: XCUIElementTypeCode.icon, frame: e.frame)
            icon.children = [badge]
            var field = RawElement(elementType: XCUIElementTypeCode.textField, frame: e.frame)
            field.identifier = "Utilities, Folder name"
            e.children += [icon, field]
        }
        #expect(SystemSurface.up(inSpringBoard: widget) == nil)
        // The real edit mode is still recognised next to an icon named "DeleteButton".
        #expect(SystemSurface.up(inSpringBoard: try forgedIcons(RealHomeScreenModeTrees.jiggle.text)) == .homeEditMode)
        #expect(SystemSurface.up(inSpringBoard: try RawTreeText.parse(RealHomeScreenModeTrees.appLibrary.text)) == .appLibrary)
        #expect(SystemSurface.up(inSpringBoard: try RawTreeText.parse(RealHomeScreenModeTrees.folder.text)) == .openFolder)
    }

    @Test("edit mode wins over the other states; the surfaces over an app still come first")
    func order() {
        let all = Set(SystemSurface.allCases.map(\.identifier))
        #expect(SystemSurface.up(present: all, lockScreen: false) == .controlCentre)
        #expect(SystemSurface.up(present: ["DeleteButton", "dewey-pod-view", ", Folder name"], lockScreen: false) == .homeEditMode)
        #expect(SystemSurface.up(present: ["dewey-pod-view", ", Folder name"], lockScreen: false) == .appLibrary)
        #expect(SystemSurface.up(present: [", Folder name"], lockScreen: false) == .openFolder)
        #expect(SystemSurface.openFolder.matches("Utilities, Folder name"))
        #expect(!SystemSurface.openFolder.matches("Utilities"))
        #expect(!SystemSurface.homeEditMode.matches("DeleteButtonX"))
    }

    @Test("the notes name the state, never its content; Home leaves it")
    func notes() {
        #expect(SystemSurface.homeEditMode.note(over: nil).hasPrefix("the Home Screen is in edit mode"))
        #expect(SystemSurface.appLibrary.note(over: nil).hasPrefix("the App Library is up"))
        #expect(SystemSurface.openFolder.note(over: nil).hasPrefix("a folder is open on the Home Screen"))
        for s in [SystemSurface.homeEditMode, .appLibrary, .openFolder] {
            let note = s.note(over: nil)
            #expect(note.contains("Home leaves it"))
            for word in ["Utilities", "Edit", "Done", "Fitness", "Other", "System", "Wall Street", "Calendar"] {
                #expect(!note.contains(word), "\(s): \(word)")
            }
        }
    }
}
