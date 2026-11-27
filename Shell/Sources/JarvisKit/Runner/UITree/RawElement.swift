import Foundation
import JarvisCore

/// One accessibility element as an in-process UI-test runner reads it (an
/// `XCUIElementSnapshot` copied field by field), before it becomes the core's `UiNode`.
///
/// A plain value type so that the conversion (`UiTreeBuilder`) runs and is tested on Linux;
/// only the copy from the snapshot lives in the Apple-only UI-test target.
public struct RawElement: Sendable, Equatable {
    /// `XCUIElement.ElementType.rawValue` (see `XCUIElementTypeCode`).
    public var elementType: UInt
    /// `label` (empty when none).
    public var label: String
    /// `value` rendered as text (`String`, or a number's description), `nil` when none.
    public var value: String?
    /// `identifier` (empty when none).
    public var identifier: String
    /// `placeholderValue`.
    public var placeholder: String?
    /// `title` (empty when none; rarely set on iOS).
    public var title: String
    /// `frame` in screen points.
    public var frame: Rect
    /// `isEnabled`.
    public var isEnabled: Bool
    /// `isSelected`.
    public var isSelected: Bool
    /// Keyboard/UI focus, when the runner could read it (`nil`: unknown).
    public var hasFocus: Bool?
    /// `children`, in document order.
    public var children: [RawElement]
    /// Review pass 31 (L1): this element had children the copy left out — it lies at the
    /// depth bound (`UiTreeBuilder.maxDepth`, the core's `MAX_TREE_DEPTH`). `UiTreeBuilder`
    /// turns it into `Result.treeCut`, so the cut reaches the core flagged.
    public var childrenCut: Bool

    /// Creates an element.
    public init(
        elementType: UInt,
        label: String = "",
        value: String? = nil,
        identifier: String = "",
        placeholder: String? = nil,
        title: String = "",
        frame: Rect,
        isEnabled: Bool = true,
        isSelected: Bool = false,
        hasFocus: Bool? = nil,
        children: [RawElement] = [],
        childrenCut: Bool = false
    ) {
        self.elementType = elementType
        self.label = label
        self.value = value
        self.identifier = identifier
        self.placeholder = placeholder
        self.title = title
        self.frame = frame
        self.isEnabled = isEnabled
        self.isSelected = isSelected
        self.hasFocus = hasFocus
        self.children = children
        self.childrenCut = childrenCut
    }

    /// Review pass 31 (L1): copies the tree under `root` (an `XCUIElementSnapshot` in the
    /// runner) into `RawElement`s without recursion, at most `maxDepth` levels deep (the root
    /// is level 1): an element at the bound keeps no children and, when it had some, says so
    /// (`childrenCut`). `element` copies one node's own fields (its `children` are ignored).
    /// The runner's first copy (`SnapshotCopy.copy`) goes through this, so no later walk of
    /// the shell ever sees a deeper tree.
    public static func copyTree<Source>(
        _ root: Source,
        maxDepth: Int = UiTreeBuilder.maxDepth,
        children: (Source) -> [Source],
        element: (Source) -> RawElement
    ) -> RawElement {
        TreeWalk.map(root, maxDepth: maxDepth, children: children) { source, kids, cut in
            var e = element(source)
            e.children = kids
            e.childrenCut = e.childrenCut || cut
            return e
        }
    }

    /// Whether some element in this subtree has focus: `true` / `false` when at least one
    /// element's focus was readable, `nil` when none was (the runner then cannot tell).
    public var subtreeFocus: Bool? {
        var readable = false
        var stack = [self]
        while let e = stack.popLast() {
            if let f = e.hasFocus {
                readable = true
                if f { return true }
            }
            stack.append(contentsOf: e.children)
        }
        return readable ? false : nil
    }

    /// Number of elements in this subtree (itself included). Iterative.
    public var count: Int {
        var n = 0
        var stack = [self]
        while let e = stack.popLast() {
            n += 1
            stack.append(contentsOf: e.children)
        }
        return n
    }
}

/// One open node of `TreeWalk.map`.
struct TreeWalkFrame<Source, Mapped> {
    let source: Source
    let pending: [Source]
    var next: Int
    var done: [Mapped]
    let cut: Bool
}

/// Review pass 31 (L1): a tree mapped without recursion on its depth, so no input can exhaust
/// a thread's stack (iOS and macOS secondary threads have 512 KB; the recursive conversion of
/// a 300-deep tree overflowed one, run 151).
public enum TreeWalk {
    /// Maps the tree under `root` bottom-up: `make(node, its mapped children, cut)` is called
    /// for every node at most `maxDepth` levels deep (the root is level 1), after its
    /// children; a node at the bound gets no children and `cut` is whether it had some.
    /// Its frames are an explicit stack: memory grows with the depth, the call stack does not.
    public static func map<Source, Mapped>(
        _ root: Source,
        maxDepth: Int,
        children: (Source) -> [Source],
        make: (Source, [Mapped], Bool) -> Mapped
    ) -> Mapped {
        typealias Frame = TreeWalkFrame<Source, Mapped>
        func open(_ s: Source, level: Int) -> Frame {
            let kids = children(s)
            if level >= maxDepth {
                return Frame(source: s, pending: [], next: 0, done: [], cut: !kids.isEmpty)
            }
            var done: [Mapped] = []
            done.reserveCapacity(kids.count)
            return Frame(source: s, pending: kids, next: 0, done: done, cut: false)
        }
        var stack: [Frame] = [open(root, level: 1)]
        while true {
            let top = stack.count - 1
            if stack[top].next < stack[top].pending.count {
                let child = stack[top].pending[stack[top].next]
                stack[top].next += 1
                stack.append(open(child, level: stack.count + 1))
                continue
            }
            let f = stack.removeLast()
            let node = make(f.source, f.done, f.cut)
            if stack.isEmpty { return node }
            stack[stack.count - 1].done.append(node)
        }
    }
}

/// `XCUIElement.ElementType` raw values (from `XCUIElementTypes.h`; the enum has kept these
/// numbers since Xcode 7). Kept as numbers so this file needs no XCTest.
public enum XCUIElementTypeCode {
    public static let any: UInt = 0
    public static let other: UInt = 1
    public static let application: UInt = 2
    public static let group: UInt = 3
    public static let window: UInt = 4
    public static let sheet: UInt = 5
    public static let drawer: UInt = 6
    public static let alert: UInt = 7
    public static let dialog: UInt = 8
    public static let button: UInt = 9
    public static let radioButton: UInt = 10
    public static let radioGroup: UInt = 11
    public static let checkBox: UInt = 12
    public static let disclosureTriangle: UInt = 13
    public static let popUpButton: UInt = 14
    public static let comboBox: UInt = 15
    public static let menuButton: UInt = 16
    public static let toolbarButton: UInt = 17
    public static let popover: UInt = 18
    public static let keyboard: UInt = 19
    public static let key: UInt = 20
    public static let navigationBar: UInt = 21
    public static let tabBar: UInt = 22
    public static let tabGroup: UInt = 23
    public static let toolbar: UInt = 24
    public static let statusBar: UInt = 25
    public static let table: UInt = 26
    public static let tableRow: UInt = 27
    public static let tableColumn: UInt = 28
    public static let outline: UInt = 29
    public static let outlineRow: UInt = 30
    public static let browser: UInt = 31
    public static let collectionView: UInt = 32
    public static let slider: UInt = 33
    public static let pageIndicator: UInt = 34
    public static let progressIndicator: UInt = 35
    public static let activityIndicator: UInt = 36
    public static let segmentedControl: UInt = 37
    public static let picker: UInt = 38
    public static let pickerWheel: UInt = 39
    public static let `switch`: UInt = 40
    public static let toggle: UInt = 41
    public static let link: UInt = 42
    public static let image: UInt = 43
    public static let icon: UInt = 44
    public static let searchField: UInt = 45
    public static let scrollView: UInt = 46
    public static let scrollBar: UInt = 47
    public static let staticText: UInt = 48
    public static let textField: UInt = 49
    public static let secureTextField: UInt = 50
    public static let datePicker: UInt = 51
    public static let textView: UInt = 52
    public static let menu: UInt = 53
    public static let menuItem: UInt = 54
    public static let menuBar: UInt = 55
    public static let menuBarItem: UInt = 56
    public static let map: UInt = 57
    public static let webView: UInt = 58
    public static let incrementArrow: UInt = 59
    public static let decrementArrow: UInt = 60
    public static let timeline: UInt = 61
    public static let ratingIndicator: UInt = 62
    public static let valueIndicator: UInt = 63
    public static let splitGroup: UInt = 64
    public static let splitter: UInt = 65
    public static let relevanceIndicator: UInt = 66
    public static let colorWell: UInt = 67
    public static let helpTag: UInt = 68
    public static let matte: UInt = 69
    public static let dockItem: UInt = 70
    public static let ruler: UInt = 71
    public static let rulerMarker: UInt = 72
    public static let grid: UInt = 73
    public static let levelIndicator: UInt = 74
    public static let cell: UInt = 75
    public static let layoutArea: UInt = 76
    public static let layoutItem: UInt = 77
    public static let handle: UInt = 78
    public static let stepper: UInt = 79
    public static let tab: UInt = 80
    public static let touchBar: UInt = 81
    public static let statusItem: UInt = 82

    private static let names: [UInt: String] = [
        0: "Any", 1: "Other", 2: "Application", 3: "Group", 4: "Window", 5: "Sheet", 6: "Drawer",
        7: "Alert", 8: "Dialog", 9: "Button", 10: "RadioButton", 11: "RadioGroup", 12: "CheckBox",
        13: "DisclosureTriangle", 14: "PopUpButton", 15: "ComboBox", 16: "MenuButton",
        17: "ToolbarButton", 18: "Popover", 19: "Keyboard", 20: "Key", 21: "NavigationBar",
        22: "TabBar", 23: "TabGroup", 24: "Toolbar", 25: "StatusBar", 26: "Table", 27: "TableRow",
        28: "TableColumn", 29: "Outline", 30: "OutlineRow", 31: "Browser", 32: "CollectionView",
        33: "Slider", 34: "PageIndicator", 35: "ProgressIndicator", 36: "ActivityIndicator",
        37: "SegmentedControl", 38: "Picker", 39: "PickerWheel", 40: "Switch", 41: "Toggle",
        42: "Link", 43: "Image", 44: "Icon", 45: "SearchField", 46: "ScrollView", 47: "ScrollBar",
        48: "StaticText", 49: "TextField", 50: "SecureTextField", 51: "DatePicker", 52: "TextView",
        53: "Menu", 54: "MenuItem", 55: "MenuBar", 56: "MenuBarItem", 57: "Map", 58: "WebView",
        59: "IncrementArrow", 60: "DecrementArrow", 61: "Timeline", 62: "RatingIndicator",
        63: "ValueIndicator", 64: "SplitGroup", 65: "Splitter", 66: "RelevanceIndicator",
        67: "ColorWell", 68: "HelpTag", 69: "Matte", 70: "DockItem", 71: "Ruler",
        72: "RulerMarker", 73: "Grid", 74: "LevelIndicator", 75: "Cell", 76: "LayoutArea",
        77: "LayoutItem", 78: "Handle", 79: "Stepper", 80: "Tab", 81: "TouchBar", 82: "StatusItem",
    ]

    /// The XCTest name of a raw type (`Type<n>` when unknown), for dumps.
    public static func name(_ raw: UInt) -> String {
        names[raw] ?? "Type\(raw)"
    }

    /// The core role of a raw type. Types the core has no role for become
    /// `Role.other(name:)` with the XCTest name, so nothing is silently merged.
    public static func role(_ raw: UInt) -> Role {
        switch raw {
        case button, radioButton, checkBox, popUpButton, menuButton, toolbarButton, disclosureTriangle,
             incrementArrow, decrementArrow, icon, stepper:
            // Home-screen icons are pressed like buttons.
            return .button
        case menuItem:
            // Probe run 134 (iOS 26.5): the edit menu's items are raw `MenuItem`s with no menu
            // or callout ancestor; the core recognises a menu item by this role alone (Rule P,
            // the hand-off items). It is pressed like a button there (`Role::is_menu_item`).
            return .other(name: "MenuItem")
        case link: return .link
        case textField, comboBox: return .textField
        case secureTextField: return .secureTextField
        case textView: return .textView
        case searchField: return .searchField
        case staticText: return .staticText
        case image: return .image
        case cell, tableRow, outlineRow: return .cell
        case `switch`, toggle: return .switch
        case slider: return .slider
        case pickerWheel: return .picker
        // picker, datePicker and segmentedControl stay containers (`other`): their wheels and
        // segment buttons are the controls, and a control role would hide them.
        case tab: return .tab
        case navigationBar: return .navigationBar
        case tabBar, tabGroup: return .tabBar
        case toolbar: return .toolbar
        case alert, dialog: return .alert
        case sheet, popover: return .sheet
        case keyboard: return .keyboard
        case key: return .key
        case scrollView: return .scrollView
        case table, outline: return .table
        case collectionView, grid: return .collectionView
        case webView: return .webView
        case window: return .window
        case application: return .application
        default: return .other(name: name(raw))
        }
    }
}
