import Foundation
import XCTest
import JarvisCore
import JarvisKit

/// Copies an `XCUIElementSnapshot` tree into JarvisKit's `RawElement` value tree. Only
/// public `XCUIElementAttributes` are read; everything that interprets them
/// (`UiTreeBuilder`) is in JarvisKit and tested on Linux.
enum SnapshotCopy {
    /// Deepest level copied (real trees are far shallower: 35 levels at most in the
    /// captures). Review pass 30: deeper than the core reads (`jarvis_proto::MAX_TREE_DEPTH`,
    /// 160), so a cut here is never silent — the wire refuses such a tree, and the core reads
    /// one that reaches it as an incomplete screen.
    static let maxDepth = UiTreeBuilder.maxDepth

    @MainActor
    static func copy(_ snapshot: any XCUIElementSnapshot, depth: Int = 0) -> RawElement {
        let frame = snapshot.frame
        let children = depth < maxDepth ? snapshot.children.map { copy($0, depth: depth + 1) } : []
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
            hasFocus: focus(snapshot),
            children: children
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
