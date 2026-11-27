import Foundation
import JarvisCore

/// Turns a `RawElement` tree (an in-process runner's copy of an XCUITest snapshot) into the
/// core's typed `UiNode` tree, and renders both as readable text for QA artifacts.
///
/// Rules (each is tested in `UiTreeBuilderTests`):
/// - Roles come from `XCUIElementTypeCode.role`; unknown types are `Role.other`.
/// - `label` falls back to `title`; empty strings become `nil`.
/// - Secure text fields are `secure` and never carry a value (XCUITest reports bullets).
/// - A text input whose value equals its placeholder has no value (XCUITest reports the
///   placeholder as the value of an empty field).
/// - Frames are sanitised (`Rect.sanitized`): non-finite components become 0, values are
///   clamped to ±100 000 pt, negative sizes become 0. System UI does report such frames.
/// - `visible`: non-empty frame that intersects the screen (when the screen size is known);
///   the core does the finer geometry (partly visible, under the keyboard, alerts).
/// - `hittable` equals `visible`: XCUITest snapshots carry no hittability, and the text path
///   makes the same assumption.
/// - `focused`: the runner's `hasFocus` reading, `false` when unknown.
/// - Keyboard visibility: any visible `Keyboard` element.
public enum UiTreeBuilder {
    /// The result of one conversion.
    public struct Result: Sendable, Equatable {
        /// The typed root.
        public var root: UiNode
        /// Whether a keyboard element is on screen.
        public var keyboardVisible: Bool
        /// Elements converted.
        public var nodeCount: Int
        /// Raw types that mapped to `Role.other` (name → count), for the artifacts.
        public var unmappedTypes: [String: Int]
        /// Elements whose frame had to be sanitised (non-finite, huge or negative).
        public var sanitizedFrames: Int
        /// Review pass 31 (L1): the tree was cut at `maxDepth` levels, here or by the first
        /// copy (`RawElement.childrenCut`): carried to the core (`RunnerObservation.treeCut`,
        /// `Screen.treeCut`), which reads the screen as incomplete.
        public var treeCut: Bool = false
    }

    /// Converts `raw` (normally the `Application` element). `screenSize` (points) decides
    /// visibility; pass `nil` to treat every non-empty frame as visible.
    public static func build(_ raw: RawElement, screenSize: Size?) -> Result {
        var keyboard = false
        var count = 0
        var unmapped: [String: Int] = [:]
        var sanitizedFrames = 0
        let bounds = screenSize.map { Rect(x: 0, y: 0, width: $0.width, height: $0.height).sanitized }

        var cut = false
        // Review pass 31 (L1): iterative (`TreeWalk.map`) and cut at `maxDepth` levels, so no
        // input depth can exhaust a 512 KB stack (the recursive conversion at 300 levels did,
        // on a macOS test thread: run 151).
        func convert(_ e: RawElement, _ children: [UiNode], _ cutHere: Bool) -> UiNode {
            if cutHere || e.childrenCut { cut = true }
            count += 1
            let role = XCUIElementTypeCode.role(e.elementType)
            if case .other(let name) = role { unmapped[name, default: 0] += 1 }
            let frame = e.frame.sanitized
            if frame != e.frame { sanitizedFrames += 1 }
            let visible = isVisible(frame, in: bounds)
            if role == .keyboard && visible { keyboard = true }
            let secure = role == .secureTextField
            let placeholder = nonEmpty(e.placeholder)
            var value = nonEmpty(e.value)
            if secure {
                value = nil
            } else if isTextInput(role), let v = value, v == placeholder {
                value = nil
            }
            return UiNode(
                role: role,
                label: nonEmpty(e.label) ?? nonEmpty(e.title),
                value: value,
                identifier: nonEmpty(e.identifier),
                placeholder: placeholder,
                frame: frame,
                enabled: e.isEnabled,
                visible: visible,
                hittable: visible,
                focused: e.hasFocus ?? false,
                selected: e.isSelected,
                secure: secure,
                children: children
            )
        }

        let root = TreeWalk.map(raw, maxDepth: maxDepth, children: { $0.children }, make: convert)
        return Result(root: root, keyboardVisible: keyboard, nodeCount: count, unmappedTypes: unmapped,
                      sanitizedFrames: sanitizedFrames, treeCut: cut)
    }

    /// Deepest level converted and copied (the root is level 1). Review pass 31 (L1): the
    /// core's one bound (`jarvis_proto::MAX_TREE_DEPTH`, 160; real trees stay under 40
    /// levels): a deeper tree is cut here — first in the runner's copy of the snapshot
    /// (`SnapshotCopy`, through `RawElement.copyTree`) — and the cut is flagged
    /// (`Result.treeCut`), never silent and never an error. It was 300 and recursive: a
    /// 300-deep conversion overflowed a 512 KB stack (run 151).
    public static let maxDepth = 160

    /// A readable dump of the typed tree, one node per line, two spaces per level:
    /// `Button "Sign In" id=login.submit {{16, 400}, {361, 44}} [disabled, focused]`.
    /// Invisible nodes are marked `(hidden)`.
    public static func dump(_ node: UiNode) -> String {
        var lines: [String] = []
        // Iterative (pre-order, as the recursive walk was): no depth can exhaust the stack.
        var stack: [(UiNode, Int)] = [(node, 0)]
        while let (n, depth) = stack.popLast() {
            var line = String(repeating: "  ", count: depth) + roleName(n.role)
            if let label = n.label { line += " \"\(escape(label))\"" }
            if let value = n.value { line += " value=\"\(escape(value))\"" }
            if let placeholder = n.placeholder { line += " placeholder=\"\(escape(placeholder))\"" }
            if let identifier = n.identifier { line += " id=\(escape(identifier))" }
            line += " " + frameText(n.frame)
            var flags: [String] = []
            if !n.enabled { flags.append("disabled") }
            if n.focused { flags.append("focused") }
            if n.selected { flags.append("selected") }
            if n.secure { flags.append("secure") }
            if !n.visible { flags.append("hidden") }
            if !flags.isEmpty { line += " [\(flags.joined(separator: ", "))]" }
            lines.append(line)
            for c in n.children.reversed() { stack.append((c, depth + 1)) }
        }
        return lines.joined(separator: "\n")
    }

    /// A dump of the raw snapshot copy with XCTest type names and raw numbers, so the
    /// mapping itself can be checked from the artifacts.
    public static func dump(_ raw: RawElement) -> String {
        var lines: [String] = []
        var stack: [(RawElement, Int)] = [(raw, 0)]
        while let (e, depth) = stack.popLast() {
            var line = String(repeating: "  ", count: depth)
            line += "\(XCUIElementTypeCode.name(e.elementType))(\(e.elementType))"
            if !e.label.isEmpty { line += " label=\"\(escape(e.label))\"" }
            if !e.title.isEmpty { line += " title=\"\(escape(e.title))\"" }
            if let v = e.value { line += " value=\"\(escape(v))\"" }
            if let p = e.placeholder, !p.isEmpty { line += " placeholder=\"\(escape(p))\"" }
            if !e.identifier.isEmpty { line += " id=\(escape(e.identifier))" }
            line += " " + frameText(e.frame)
            var flags: [String] = []
            if !e.isEnabled { flags.append("disabled") }
            if e.isSelected { flags.append("selected") }
            switch e.hasFocus {
            case true?: flags.append("focus")
            case nil: flags.append("focus=?")
            case false?: break
            }
            if e.childrenCut { flags.append("children cut") }
            if !flags.isEmpty { line += " [\(flags.joined(separator: ", "))]" }
            lines.append(line)
            for c in e.children.reversed() { stack.append((c, depth + 1)) }
        }
        return lines.joined(separator: "\n")
    }

    /// The core's name for a role (as the prompt renders it), `Other(<name>)` for others.
    public static func roleName(_ role: Role) -> String {
        switch role {
        case .button: "Button"
        case .link: "Link"
        case .textField: "TextField"
        case .secureTextField: "SecureTextField"
        case .textView: "TextView"
        case .searchField: "SearchField"
        case .staticText: "StaticText"
        case .image: "Image"
        case .cell: "Cell"
        case .switch: "Switch"
        case .slider: "Slider"
        case .picker: "Picker"
        case .tab: "Tab"
        case .navigationBar: "NavigationBar"
        case .tabBar: "TabBar"
        case .toolbar: "Toolbar"
        case .alert: "Alert"
        case .sheet: "Sheet"
        case .keyboard: "Keyboard"
        case .key: "Key"
        case .scrollView: "ScrollView"
        case .table: "Table"
        case .collectionView: "CollectionView"
        case .webView: "WebView"
        case .window: "Window"
        case .application: "Application"
        case .other(let name): "Other(\(name))"
        }
    }

    // MARK: - Helpers

    static func isTextInput(_ role: Role) -> Bool {
        switch role {
        case .textField, .secureTextField, .textView, .searchField: true
        default: false
        }
    }

    static func isVisible(_ frame: Rect, in bounds: Rect?) -> Bool {
        guard frame.width > 0, frame.height > 0, frame.x.isFinite, frame.y.isFinite else { return false }
        guard let b = bounds, b.width > 0, b.height > 0 else { return true }
        return frame.x < b.x + b.width && frame.x + frame.width > b.x
            && frame.y < b.y + b.height && frame.y + frame.height > b.y
    }

    private static func nonEmpty(_ s: String?) -> String? {
        guard let s, !s.isEmpty else { return nil }
        return s
    }

    private static func frameText(_ f: Rect) -> String {
        "{{\(num(f.x)), \(num(f.y))}, {\(num(f.width)), \(num(f.height))}}"
    }

    private static func num(_ v: Double) -> String {
        v.compactText
    }

    private static func escape(_ s: String) -> String {
        var out = ""
        for c in s {
            switch c {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            default: out.append(c)
            }
        }
        return out
    }
}
