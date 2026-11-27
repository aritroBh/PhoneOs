import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Copies an `XCUIElementSnapshot` tree into JarvisKit's `RawElement` value tree. Only
/// public `XCUIElementAttributes` are read; everything that interprets them
/// (`UiTreeBuilder`) is in JarvisKit and tested on Linux.
enum SnapshotCopy {
    /// Deepest level copied, the root being level 1 (real trees are far shallower: 36 levels
    /// at most in the captures). Review pass 31 (L1): the core's one bound
    /// (`jarvis_proto::MAX_TREE_DEPTH`, 160, `UiTreeBuilder.maxDepth`): a deeper tree is cut
    /// HERE, in the first copy, and the element at the bound says so (`childrenCut`, carried
    /// to the core as `treeCut`), so no later walk sees a deeper tree. The copy is iterative
    /// (`RawElement.copyTree`): it was recursive to 300 levels, past a 512 KB stack.
    static let maxDepth = UiTreeBuilder.maxDepth

    /// A snapshot as `RawElement.copyTree`'s source (a concrete type, so no existential is
    /// opened for the generic parameter).
    private struct Node {
        let snapshot: any XCUIElementSnapshot
    }

    @MainActor
    static func copy(_ snapshot: any XCUIElementSnapshot) -> RawElement {
        RawElement.copyTree(
            Node(snapshot: snapshot),
            maxDepth: maxDepth,
            children: { (n: Node) -> [Node] in n.snapshot.children.map { Node(snapshot: $0) } },
            element: { (n: Node) -> RawElement in one(n.snapshot) }
        )
    }

    /// One snapshot's own fields (no children).
    @MainActor
    private static func one(_ snapshot: any XCUIElementSnapshot) -> RawElement {
        let frame = snapshot.frame
        return RawElement(
            elementType: snapshot.elementType.rawValue,
            label: snapshot.label,
            value: text(snapshot.value),
            identifier: snapshot.identifier,
            placeholder: snapshot.placeholderValue,
            title: snapshot.title,
            frame: Rect(
                x: Double(frame.origin.x), y: Double(frame.origin.y),
                width: Double(frame.size.width), height: Double(frame.size.height)
            ),
            isEnabled: snapshot.isEnabled,
            isSelected: snapshot.isSelected,
            hasFocus: focus(snapshot)
        )
    }

    /// `value` as text: strings as they are, numbers by their description (a switch's
    /// value is `0`/`1`), anything else described.
    static func text(_ value: Any?) -> String? {
        switch value {
        case nil: return nil
        case let string as String: return string
        case let number as NSNumber: return number.stringValue
        case let other?: return String(describing: other)
        }
    }

    /// Keyboard focus, read defensively. `hasFocus` is declared on `XCUIElementAttributes`
    /// but its iOS availability is not certain from Linux, so it is read through KVC, and
    /// only after `responds(to:)` says the snapshot has it (an unknown KVC key would raise).
    /// `nil` means "could not be read".
    static func focus(_ snapshot: any XCUIElementSnapshot) -> Bool? {
        guard let object = snapshot as? NSObject else { return nil }
        for key in ["hasKeyboardFocus", "hasFocus"] where object.responds(to: NSSelectorFromString(key)) {
            if let value = object.value(forKey: key) as? Bool { return value }
            if let number = object.value(forKey: key) as? NSNumber { return number.boolValue }
        }
        return nil
    }
}
