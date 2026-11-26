import AppTrackingTransparency
import AVFoundation
import CallKit
import Contacts
import ContactsUI
import CoreLocation
import MessageUI
import Network
import Photos
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers
import UserNotifications
import WebKit

/// Screens for probe round 3 (`TreeFactProbe3Tests`, `TreeFactProbe3SystemTests`): controls,
/// modals, pickers, permission prompts and system surfaces whose real accessibility trees the
/// probes record. Reached from the Playground's row "Fact probes 3" (after "Fact probes", so
/// every earlier row keeps its place). Playground only: nothing here runs in a real task.
///
/// Labels are left as the system makes them wherever the label itself is the question (the
/// symbol-only buttons have no `accessibilityLabel` and no identifier on purpose).
enum Probe3Screen: String, Hashable, CaseIterable {
    case symbols, returns, modals, controls, web, pickers, permissions, system, swipes
    case swipesNarrow, swipesColumns, swipesLeading, swipesPlain, swipesRevealed

    var title: String {
        switch self {
        case .symbols: "Symbol buttons"
        case .returns: "Return keys"
        case .modals: "Modals"
        case .controls: "Controls 2"
        case .web: "Web view"
        case .pickers: "Pickers"
        case .permissions: "Permissions"
        case .system: "System"
        case .swipes: "Swipe rows"
        case .swipesNarrow: "Swipe rows, narrow"
        case .swipesColumns: "Swipe rows, two columns"
        case .swipesLeading: "Swipe rows, leading"
        case .swipesPlain: "Swipe rows, edge to edge"
        case .swipesRevealed: "Swipe rows, revealed"
        }
    }
}

struct Probes3Index: View {
    var body: some View {
        List {
            ForEach(Probe3Screen.allCases, id: \.self) { screen in
                NavigationLink(value: PlaygroundView.Route.probe3(screen)) {
                    Text(screen.title)
                }
                .accessibilityIdentifier("probe3.\(screen.rawValue)")
            }
        }
        .navigationTitle("Fact probes 3")
    }
}

struct Probe3Destination: View {
    let screen: Probe3Screen

    var body: some View {
        switch screen {
        case .symbols: Probe3SymbolsFixture()
        case .returns: Probe3ReturnKeysFixture()
        case .modals: Probe3ModalsFixture()
        case .controls: Probe3ControlsFixture()
        case .web: Probe3WebViewFixture()
        case .pickers: Probe3PickersFixture()
        case .permissions: Probe3PermissionsFixture()
        case .system: Probe3SystemFixture()
        case .swipes: Probe3SwipeRowsFixture()
        case .swipesNarrow: Probe3SwipeVariantFixture(variant: .narrow)
        case .swipesColumns: Probe3SwipeVariantFixture(variant: .columns)
        case .swipesLeading: Probe3SwipeVariantFixture(variant: .leading)
        case .swipesPlain: Probe3SwipeVariantFixture(variant: .plain)
        case .swipesRevealed: Probe3SwipeRevealedFixture()
        }
    }
}

// MARK: - P4: symbol-only buttons (SwiftUI, UIKit, toolbar system items)

/// Symbol-only buttons with no explicit label or identifier, in three groups introduced by a
/// header text each (the probe attributes buttons to symbols by the headers and the order):
/// SwiftUI `Button { } label: { Image(systemName:) }`, UIKit `UIButton` with the symbol image,
/// and `UIToolbar`s of `UIBarButtonItem(barButtonSystemItem:)`. A text field on top gives the
/// probes a keyboard (P10/P11).
struct Probe3SymbolsFixture: View {
    static let symbols = [
        "trash", "paperplane", "paperplane.fill", "cart", "creditcard",
        "phone", "phone.fill", "square.and.arrow.up", "xmark", "arrow.up.circle.fill",
        "checkmark", "plus", "minus.circle", "bag", "person.crop.circle.badge.plus",
    ]
    static let firstItems: [UIBarButtonItem.SystemItem] = [.trash, .action, .compose, .reply]
    static let secondItems: [UIBarButtonItem.SystemItem] = [.add, .done, .cancel, .save]
    /// The order of `firstItems` then `secondItems`, as the probe names them.
    static let itemNames = ["trash", "action", "compose", "reply", "add", "done", "cancel", "save"]

    @State private var text = ""
    @State private var tapped = "Nothing tapped"

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                TextField("Probe symbols field", text: $text)
                    .textFieldStyle(.roundedBorder)
                    .autocorrectionDisabled(true)
                    .textInputAutocapitalization(.never)
                    .accessibilityIdentifier("probe3.symbols.field")
                Text("SwiftUI symbol buttons")
                ForEach(0..<3, id: \.self) { row in
                    HStack(spacing: 12) {
                        ForEach(Array(Self.symbols[(row * 5)..<(row * 5 + 5)]), id: \.self) { name in
                            Button {
                                tapped = "Tapped \(name)"
                            } label: {
                                Image(systemName: name)
                                    .frame(width: 44, height: 44)
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                }
                Text("UIKit symbol buttons")
                Probe3UIKitSymbolButtons(symbols: Self.symbols)
                    .frame(height: 3 * 46)
                Text("UIKit toolbar items")
                Probe3SystemItemToolbar(items: Self.firstItems)
                    .frame(height: 50)
                Probe3SystemItemToolbar(items: Self.secondItems)
                    .frame(height: 50)
                Text(tapped)
                    .accessibilityIdentifier("probe3.symbols.status")
            }
            .padding()
        }
        .navigationTitle("Symbols")
    }
}

/// Rows of five `UIButton(type: .system)`s, each with only a symbol image.
private struct Probe3UIKitSymbolButtons: UIViewRepresentable {
    let symbols: [String]

    func makeUIView(context: Context) -> UIStackView {
        let column = UIStackView()
        column.axis = .vertical
        column.spacing = 2
        column.distribution = .fillEqually
        var index = 0
        while index < symbols.count {
            let row = UIStackView()
            row.axis = .horizontal
            row.spacing = 8
            row.distribution = .fillEqually
            for name in symbols[index..<min(index + 5, symbols.count)] {
                let button = UIButton(type: .system)
                button.setImage(UIImage(systemName: name), for: .normal)
                row.addArrangedSubview(button)
            }
            column.addArrangedSubview(row)
            index += 5
        }
        return column
    }

    func updateUIView(_ uiView: UIStackView, context: Context) {}
}

/// A `UIToolbar` of system items (flexible space between them); the items do nothing.
private struct Probe3SystemItemToolbar: UIViewRepresentable {
    let items: [UIBarButtonItem.SystemItem]

    func makeUIView(context: Context) -> UIToolbar {
        let bar = UIToolbar(frame: CGRect(x: 0, y: 0, width: 400, height: 44))
        var list: [UIBarButtonItem] = []
        for item in items {
            if !list.isEmpty {
                list.append(UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil))
            }
            list.append(UIBarButtonItem(barButtonSystemItem: item, target: nil, action: nil))
        }
        bar.setItems(list, animated: false)
        return bar
    }

    func updateUIView(_ uiView: UIToolbar, context: Context) {}
}

// MARK: - P4: every UIReturnKeyType

/// One UIKit text field whose `returnKeyType` steps through every `UIReturnKeyType` ("Next
/// return type"); the status shows which one is set. The probe ends editing, steps, and
/// focuses the field again for each type, so the keyboard is built afresh every time.
struct Probe3ReturnKeysFixture: View {
    static let types: [(String, UIReturnKeyType)] = [
        ("default", .default), ("go", .go), ("google", .google), ("join", .join), ("next", .next),
        ("route", .route), ("search", .search), ("send", .send), ("yahoo", .yahoo), ("done", .done),
        ("emergencyCall", .emergencyCall), ("continue", UIReturnKeyType.continue),
    ]

    @State private var index = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Return type \(index + 1) of \(Self.types.count): \(Self.types[index].0)")
                .accessibilityIdentifier("probe3.returns.status")
            HStack {
                Button("Next return type") { index = (index + 1) % Self.types.count }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("probe3.returns.next")
                Spacer()
                Button("End editing") {
                    _ = UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("probe3.returns.end")
            }
            Probe3ReturnKeyField(name: Self.types[index].0, type: Self.types[index].1)
                .frame(height: 44)
            Spacer()
        }
        .padding()
        .navigationTitle("Return keys")
    }
}

private struct Probe3ReturnKeyField: UIViewRepresentable {
    let name: String
    let type: UIReturnKeyType

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.borderStyle = .roundedRect
        field.autocorrectionType = .no
        field.autocapitalizationType = .none
        field.spellCheckingType = .no
        field.accessibilityIdentifier = "probe3.returns.field"
        field.returnKeyType = type
        field.placeholder = "Return \(name)"
        return field
    }

    func updateUIView(_ field: UITextField, context: Context) {
        guard field.returnKeyType != type else { return }
        field.returnKeyType = type
        field.placeholder = "Return \(name)"
        if field.isFirstResponder { field.reloadInputViews() }
    }
}

// MARK: - P4: modals

/// An action sheet (`confirmationDialog`), an alert with a text field, a full sheet, a half
/// sheet, a full-screen cover, a popover, a `Menu` and a context menu, each with a destructive
/// action where one fits. "Probe content behind" stays on the screen under every modal.
struct Probe3ModalsFixture: View {
    @State private var status = "No modal yet"
    @State private var dialog = false
    @State private var alert = false
    @State private var alertText = ""
    @State private var sheet = false
    @State private var halfSheet = false
    @State private var cover = false
    @State private var popover = false

    var body: some View {
        VStack(spacing: 0) {
            // Runs 146 and 149: inside the Form's row the Menu never opened (a coordinate tap
            // at the row's centre; then XCUITest found its label "not hittable", hit point
            // {-1, -1}). A plain bordered Menu above the Form, outside any List row.
            HStack {
                Menu("Probe menu") {
                    Button("Probe menu item") { status = "Menu item chosen" }
                    Button("Delete from menu", role: .destructive) { status = "Deleted from the menu" }
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("probe3.modals.menu")
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 8)
            Form {
            Text("Probe content behind")
                .accessibilityIdentifier("probe3.modals.behind")
            Button("Show action sheet") { dialog = true }
                .accessibilityIdentifier("probe3.modals.dialog")
                .confirmationDialog("Probe action sheet", isPresented: $dialog, titleVisibility: .visible) {
                    Button("Delete item", role: .destructive) { status = "Deleted from the action sheet" }
                    Button("Keep item") { status = "Kept from the action sheet" }
                    Button("Cancel", role: .cancel) { status = "Action sheet cancelled" }
                } message: {
                    Text("Probe action sheet message")
                }
            Button("Show alert with field") { alert = true }
                .accessibilityIdentifier("probe3.modals.alert")
                .alert("Probe alert with field", isPresented: $alert) {
                    TextField("Probe alert field", text: $alertText)
                    Button("Delete", role: .destructive) { status = "Deleted from the alert" }
                    Button("Cancel", role: .cancel) { status = "Alert cancelled" }
                } message: {
                    Text("Probe alert message")
                }
            Button("Show sheet") { sheet = true }
                .accessibilityIdentifier("probe3.modals.sheet")
                .sheet(isPresented: $sheet) {
                    Probe3SheetContent(title: "Probe full sheet") { sheet = false }
                        .presentationDetents([.large])
                }
            Button("Show half sheet") { halfSheet = true }
                .accessibilityIdentifier("probe3.modals.half")
                .sheet(isPresented: $halfSheet) {
                    Probe3SheetContent(title: "Probe half sheet") { halfSheet = false }
                        .presentationDetents([.medium])
                }
            Button("Show cover") { cover = true }
                .accessibilityIdentifier("probe3.modals.cover")
                .fullScreenCover(isPresented: $cover) {
                    Probe3SheetContent(title: "Probe cover") { cover = false }
                }
            Button("Show popover") { popover = true }
                .accessibilityIdentifier("probe3.modals.popover")
                .popover(isPresented: $popover) {
                    Probe3SheetContent(title: "Probe popover") { popover = false }
                        .presentationCompactAdaptation(.popover)
                }
            Text("Hold for the probe context menu")
                .frame(maxWidth: .infinity, minHeight: 60)
                .contentShape(Rectangle())
                .contextMenu {
                    Button("Probe context item") { status = "Context item chosen" }
                    Button("Delete from context menu", role: .destructive) { status = "Deleted from the context menu" }
                }
                .accessibilityIdentifier("probe3.modals.context")
            Text(status)
                .accessibilityIdentifier("probe3.modals.status")
            }
        }
        .navigationTitle("Modals")
    }
}

private struct Probe3SheetContent: View {
    let title: String
    let close: () -> Void

    var body: some View {
        VStack(spacing: 24) {
            Text(title)
                .font(.title2)
            Text("\(title) body text")
            Button("Close \(title)") { close() }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("probe3.modals.close")
        }
        .padding()
    }
}

// MARK: - P4: controls round 2

/// Swipe rows (a destructive full-swipe action), a segmented picker, a `Link`, a
/// `NavigationLink`, a three-tab `TabView` (in a cover), a stepper, compact and wheel date
/// pickers, a toggle, an empty and a filled secure field, an inline picker and rows marked
/// selected with a checkmark.
struct Probe3ControlsFixture: View {
    private static let linkURL = URL(string: "https://example.com/probe-link") ?? URL(fileURLWithPath: "/")

    @State private var rows = [1, 2, 3]
    @State private var segment = "Two"
    @State private var count = 2
    @State private var date = Date(timeIntervalSince1970: 1_800_000_000)
    @State private var on = true
    @State private var emptySecret = ""
    @State private var filledSecret = "probe-secret-1"
    @State private var inlineChoice = "Beta"
    @State private var checked = 2
    @State private var status = "No action"
    @State private var tabs = false

    var body: some View {
        Form {
            Section("Swipe rows") {
                ForEach(rows, id: \.self) { n in
                    Text("Swipe row \(n)")
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                rows.removeAll { $0 == n }
                                status = "Deleted swipe row \(n)"
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                            Button {
                                status = "Flagged swipe row \(n)"
                            } label: {
                                Label("Flag", systemImage: "flag")
                            }
                        }
                }
            }
            Section {
                Text(status)
                    .accessibilityIdentifier("probe3.controls.status")
            }
            Section("Segmented and links") {
                Picker("Probe segment", selection: $segment) {
                    Text("One").tag("One")
                    Text("Two").tag("Two")
                    Text("Three").tag("Three")
                }
                .pickerStyle(.segmented)
                Link("Probe link", destination: Self.linkURL)
                NavigationLink("Probe navigation link") {
                    Text("Probe destination")
                        .navigationTitle("Destination")
                }
                Button("Show tabs") { tabs = true }
                    .accessibilityIdentifier("probe3.controls.tabs")
            }
            Section("Values") {
                Stepper("Probe stepper \(count)", value: $count, in: 0...9)
                DatePicker("Probe compact date", selection: $date, displayedComponents: [.date])
                    .datePickerStyle(.compact)
                    .accessibilityIdentifier("probe3.controls.compactdate")
                Toggle("Probe toggle", isOn: $on)
                SecureField("Probe empty secret", text: $emptySecret)
                SecureField("Probe filled secret", text: $filledSecret)
                DatePicker("Probe wheel time", selection: $date, displayedComponents: [.hourAndMinute])
                    .datePickerStyle(.wheel)
            }
            Section("Selection") {
                Picker("Probe inline choice", selection: $inlineChoice) {
                    Text("Alpha").tag("Alpha")
                    Text("Beta").tag("Beta")
                    Text("Gamma").tag("Gamma")
                }
                .pickerStyle(.inline)
                ForEach(1...3, id: \.self) { n in
                    Button {
                        checked = n
                    } label: {
                        HStack {
                            Text("Check row \(n)")
                            Spacer()
                            if checked == n {
                                Image(systemName: "checkmark")
                            }
                        }
                    }
                    .accessibilityAddTraits(checked == n ? AccessibilityTraits.isSelected : [])
                }
            }
        }
        .navigationTitle("Controls 2")
        .fullScreenCover(isPresented: $tabs) {
            Probe3TabsCover { tabs = false }
        }
    }
}

private struct Probe3TabsCover: View {
    let close: () -> Void
    @State private var tab = 1

    var body: some View {
        TabView(selection: $tab) {
            ForEach(1...3, id: \.self) { n in
                VStack(spacing: 20) {
                    Text("Probe tab content \(n)")
                    Button("Close tabs") { close() }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("probe3.tabs.close")
                }
                .tabItem { Label("Probe tab \(n)", systemImage: "\(n).circle") }
                .tag(n)
            }
        }
    }
}

// MARK: - P13: swipe lengths

/// Twenty-four rows whose trailing swipe actions (a destructive Delete, a Flag) allow a full
/// swipe, under a status line. P13a drags each row once, with a different path, and reads
/// whether the row was deleted, its actions revealed, or nothing happened.
struct Probe3SwipeRowsFixture: View {
    @State private var rows = Array(1...24)
    @State private var status = "No swipe action yet"

    var body: some View {
        List {
            Section {
                Text(status)
                    .accessibilityIdentifier("probe3.swipes.status")
            }
            Section("Full swipe rows") {
                ForEach(rows, id: \.self) { n in
                    Text("Full swipe row \(n)")
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                rows.removeAll { $0 == n }
                                status = "Deleted full swipe row \(n)"
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                            Button {
                                status = "Flagged full swipe row \(n)"
                            } label: {
                                Label("Flag", systemImage: "flag")
                            }
                        }
                }
            }
        }
        .navigationTitle("Swipe rows")
    }
}

enum Probe3SwipeVariant {
    case narrow, columns, leading, plain
}

/// P13b: rows that are not full width (a plain list 300 pt wide; two plain lists 200 pt
/// wide side by side, "Left row N" and "Right row N"), each row with trailing Delete
/// (destructive, full swipe) and Flag; or full-width rows with a leading full-swipe action
/// (Remove, destructive; Pin). One status line on top.
struct Probe3SwipeVariantFixture: View {
    let variant: Probe3SwipeVariant
    @State private var rows = Array(1...12)
    @State private var right = Array(1...8)
    @State private var status = "No swipe action yet"

    var body: some View {
        VStack(spacing: 0) {
            Text(status)
                .accessibilityIdentifier("probe3.swipevariant.status")
                .padding(.vertical, 8)
            switch variant {
            case .narrow:
                trailingList("Narrow row", $rows)
                    .frame(width: 300)
            case .plain:
                trailingList("Plain row", $rows)
            case .columns:
                HStack(spacing: 8) {
                    trailingList("Left row", $rows)
                        .frame(width: 200)
                    trailingList("Right row", $right)
                        .frame(width: 200)
                }
            case .leading:
                List {
                    ForEach(rows, id: \.self) { n in
                        Text("Leading row \(n)")
                            .swipeActions(edge: .leading, allowsFullSwipe: true) {
                                Button(role: .destructive) {
                                    rows.removeAll { $0 == n }
                                    status = "Removed leading row \(n)"
                                } label: {
                                    Label("Remove", systemImage: "trash")
                                }
                                Button {
                                    status = "Pinned leading row \(n)"
                                } label: {
                                    Label("Pin", systemImage: "pin")
                                }
                            }
                    }
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle("Swipe variants")
    }

    private func trailingList(_ prefix: String, _ list: Binding<[Int]>) -> some View {
        List {
            ForEach(list.wrappedValue, id: \.self) { n in
                Text("\(prefix) \(n)")
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        Button(role: .destructive) {
                            list.wrappedValue.removeAll { $0 == n }
                            status = "Deleted \(prefix.lowercased()) \(n)"
                        } label: {
                            Label("Delete", systemImage: "trash")
                        }
                        Button {
                            status = "Flagged \(prefix.lowercased()) \(n)"
                        } label: {
                            Label("Flag", systemImage: "flag")
                        }
                    }
            }
        }
        .listStyle(.plain)
    }
}

/// The row kinds of the Swipe rows, revealed screen, chosen by its segmented picker.
enum Probe3RevealKind: String, CaseIterable, Hashable {
    case insetOne, edgeOne, insetTwo, leadingOne, taps

    /// The picker's segment title (what the probe taps).
    var segment: String {
        switch self {
        case .insetOne: "Inset 1"
        case .edgeOne: "Edge 1"
        case .insetTwo: "Inset 2"
        case .leadingOne: "Leading"
        case .taps: "Taps"
        }
    }

    /// The rows' text: "<prefix> N".
    var prefix: String {
        switch self {
        case .insetOne: "Inset one row"
        case .edgeOne: "Edge one row"
        case .insetTwo: "Inset two row"
        case .leadingOne: "Leading one row"
        case .taps: "Tap row"
        }
    }

    /// An edge-to-edge (plain) list; else an inset grouped one (400 pt rows on a 440 pt screen).
    var plain: Bool {
        self == .edgeOne || self == .leadingOne
    }

    var edge: HorizontalEdge {
        self == .leadingOne ? .leading : .trailing
    }

    /// The destructive action's title (it removes the row).
    var removeTitle: String {
        self == .leadingOne ? "Remove" : "Delete"
    }

    /// A Flag action after the destructive one.
    var twoActions: Bool {
        self == .insetTwo || self == .taps
    }
}

/// P13e, P13f, P13g: sixteen fresh rows of each kind, under a status line and a segmented
/// picker of the kind: one trailing Delete on 400 pt rows (Inset 1) and on 440 pt edge-to-edge
/// rows (Edge 1); trailing Delete and Flag on 400 pt rows (Inset 2); one leading Remove on
/// 440 pt rows (Leading); and rows that are buttons ("Opened tap row N") with trailing Delete
/// and Flag (Taps). Every destructive action allows a full swipe and removes its row at once.
/// A new screen, so the earlier P13 screens and their rows stay as they were.
struct Probe3SwipeRevealedFixture: View {
    @State private var kind = Probe3RevealKind.insetOne
    @State private var rows: [Probe3RevealKind: [Int]] = Dictionary(uniqueKeysWithValues: Probe3RevealKind.allCases.map { ($0, Array(1...16)) })
    @State private var status = "No swipe action yet"

    var body: some View {
        VStack(spacing: 0) {
            Text(status)
                .accessibilityIdentifier("probe3.swiperevealed.status")
                .padding(.vertical, 8)
            Picker("Row kind", selection: $kind) {
                ForEach(Probe3RevealKind.allCases, id: \.self) { k in
                    Text(k.segment).tag(k)
                }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
            if kind.plain {
                list(kind)
                    .listStyle(.plain)
            } else {
                list(kind)
                    .listStyle(.insetGrouped)
            }
        }
        .navigationTitle("Swipe rows, revealed")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func list(_ k: Probe3RevealKind) -> some View {
        List {
            ForEach(rows[k] ?? [], id: \.self) { n in
                row(k, n)
                    .swipeActions(edge: k.edge, allowsFullSwipe: true) {
                        Button(role: .destructive) {
                            rows[k, default: []].removeAll { $0 == n }
                            status = "\(k.removeTitle)d \(k.prefix.lowercased()) \(n)"
                        } label: {
                            Label(k.removeTitle, systemImage: "trash")
                        }
                        if k.twoActions {
                            Button {
                                status = "Flagged \(k.prefix.lowercased()) \(n)"
                            } label: {
                                Label("Flag", systemImage: "flag")
                            }
                        }
                    }
            }
        }
        .id(k)
    }

    @ViewBuilder
    private func row(_ k: Probe3RevealKind, _ n: Int) -> some View {
        if k == .taps {
            Button("\(k.prefix) \(n)") {
                status = "Opened \(k.prefix.lowercased()) \(n)"
            }
        } else {
            Text("\(k.prefix) \(n)")
        }
    }
}

// MARK: - P6: a WKWebView in the app

/// The probe's local page (`-jarvis-probe-url`) in a `WKWebView` under a row of the host's own
/// chrome (a title and a button), so the tree shows whether the two can be told apart. No
/// `WKUIDelegate`: JavaScript dialogs are not shown here (recorded, not probed).
struct Probe3WebViewFixture: View {
    @State private var reloads = 0

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Text("Host chrome")
                    .font(.headline)
                    .accessibilityIdentifier("probe3.web.chrome")
                Spacer()
                Button("Host button \(reloads)") { reloads += 1 }
                    .accessibilityIdentifier("probe3.web.hostbutton")
            }
            .padding(.horizontal)
            Probe3WebView(url: ProbePresenter.probeURL)
        }
        .navigationTitle("Web view")
    }
}

private struct Probe3WebView: UIViewRepresentable {
    let url: URL?

    func makeUIView(context: Context) -> WKWebView {
        let view = WKWebView(frame: .zero, configuration: WKWebViewConfiguration())
        if let url { view.load(URLRequest(url: url)) }
        return view
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}
}

// MARK: - P2: out-of-process pickers and the message composer

/// Keeps the pickers' delegates alive (delegates are weak) and dismisses the controllers that
/// do not dismiss themselves (the photo picker, the message composer).
@MainActor
final class Probe3PickerDelegates: NSObject, @preconcurrency PHPickerViewControllerDelegate,
    @preconcurrency CNContactPickerDelegate, @preconcurrency UIDocumentPickerDelegate,
    @preconcurrency MFMessageComposeViewControllerDelegate {
    static let shared = Probe3PickerDelegates()

    func photoPicker() -> PHPickerViewController {
        var configuration = PHPickerConfiguration()
        configuration.selectionLimit = 1
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = self
        return picker
    }

    func contactPicker() -> CNContactPickerViewController {
        let picker = CNContactPickerViewController()
        picker.delegate = self
        return picker
    }

    func documentPicker() -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [UTType.item])
        picker.delegate = self
        return picker
    }

    func messageComposer() -> MFMessageComposeViewController {
        let composer = MFMessageComposeViewController()
        composer.recipients = ["5555550123"]
        composer.body = "Probe message body"
        composer.messageComposeDelegate = self
        return composer
    }

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        picker.dismiss(animated: true)
    }

    func contactPickerDidCancel(_ picker: CNContactPickerViewController) {}

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {}

    func messageComposeViewController(_ controller: MFMessageComposeViewController, didFinishWith result: MessageComposeResult) {
        controller.dismiss(animated: true)
    }
}

struct Probe3PickersFixture: View {
    @State private var status = "Idle"

    var body: some View {
        Form {
            Text("Probe host content")
                .accessibilityIdentifier("probe3.pickers.behind")
            Button("Photo picker") {
                ProbePresenter.present(Probe3PickerDelegates.shared.photoPicker())
                status = "Photo picker presented"
            }
            .accessibilityIdentifier("probe3.pickers.photos")
            Button("Contact picker") {
                ProbePresenter.present(Probe3PickerDelegates.shared.contactPicker())
                status = "Contact picker presented"
            }
            .accessibilityIdentifier("probe3.pickers.contacts")
            Button("Document picker") {
                ProbePresenter.present(Probe3PickerDelegates.shared.documentPicker())
                status = "Document picker presented"
            }
            .accessibilityIdentifier("probe3.pickers.documents")
            Button("Message composer") {
                if MFMessageComposeViewController.canSendText() {
                    ProbePresenter.present(Probe3PickerDelegates.shared.messageComposer())
                    status = "Message composer presented"
                } else {
                    status = "canSendText false: no composer"
                }
            }
            .accessibilityIdentifier("probe3.pickers.message")
            Text(status)
                .accessibilityIdentifier("probe3.pickers.status")
        }
        .navigationTitle("Pickers")
    }
}

// MARK: - P9: permission prompts

/// Asks for one permission per button (each asked once per simulator: the probe denies it).
@MainActor
final class Probe3PermissionAsker: ObservableObject {
    @Published var status = "Nothing asked"
    private let locationManager = CLLocationManager()
    private var browser: NWBrowser?

    func askLocation() {
        locationManager.requestWhenInUseAuthorization()
        status = "location asked (status \(locationManager.authorizationStatus.rawValue) when asked)"
    }

    func askCamera() {
        status = "Asking camera"
        Task { @MainActor in
            let granted = await AVCaptureDevice.requestAccess(for: .video)
            self.status = "camera granted \(granted)"
        }
    }

    func askMicrophone() {
        status = "Asking microphone"
        Task { @MainActor in
            let granted = await AVCaptureDevice.requestAccess(for: .audio)
            self.status = "microphone granted \(granted)"
        }
    }

    func askContacts() {
        status = "Asking contacts"
        Task { @MainActor in
            do {
                let granted = try await CNContactStore().requestAccess(for: .contacts)
                self.status = "contacts granted \(granted)"
            } catch {
                self.status = "contacts failed: \(error.localizedDescription)"
            }
        }
    }

    func askPhotos() {
        status = "Asking photos"
        Task { @MainActor in
            let answer = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
            self.status = "photos status \(answer.rawValue)"
        }
    }

    func askTracking() {
        status = "Asking tracking"
        Task { @MainActor in
            let answer = await ATTrackingManager.requestTrackingAuthorization()
            self.status = "tracking status \(answer.rawValue)"
        }
    }

    /// Browses `_jarvis-remote._tcp` (listed in NSBonjourServices; NSLocalNetworkUsageDescription
    /// is set) and sends one mDNS query by UDP to 224.0.0.251:5353, the two attempts that
    /// raise the local network prompt on a device. The status line keeps each state the
    /// browser and the connection report (run 149: no prompt within 10 s).
    func browseLocalNetwork() {
        browser?.cancel()
        connection?.cancel()
        let started = NWBrowser(for: .bonjour(type: "_jarvis-remote._tcp", domain: nil), using: .tcp)
        started.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated { self?.noteNetwork("browser \(state)") }
        }
        started.browseResultsChangedHandler = { [weak self] results, _ in
            MainActor.assumeIsolated { self?.noteNetwork("browser results \(results.count)") }
        }
        started.start(queue: .main)
        browser = started
        let udp = NWConnection(host: NWEndpoint.Host("224.0.0.251"), port: NWEndpoint.Port(rawValue: 5353) ?? .any, using: .udp)
        udp.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated { self?.noteNetwork("udp \(state)") }
        }
        udp.start(queue: .main)
        // A minimal mDNS query for _jarvis-remote._tcp.local (PTR).
        var query: [UInt8] = [0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]
        for label in ["_jarvis-remote", "_tcp", "local"] {
            query.append(UInt8(label.utf8.count))
            query.append(contentsOf: Array(label.utf8))
        }
        query.append(contentsOf: [0, 0, 12, 0, 1])
        udp.send(content: Data(query), completion: .contentProcessed { [weak self] error in
            MainActor.assumeIsolated { self?.noteNetwork("udp send \(error.map { "\($0)" } ?? "ok")") }
        })
        connection = udp
        network = []
        status = "local network browse started"
    }

    private var network: [String] = []
    private var connection: NWConnection?

    private func noteNetwork(_ event: String) {
        network.append(event)
        status = "local network: " + network.suffix(6).joined(separator: "; ")
    }

    func stopLocalNetwork() {
        browser?.cancel()
        browser = nil
        connection?.cancel()
        connection = nil
    }

    func readPasteboard() {
        let text = UIPasteboard.general.string
        status = "pasteboard " + (text.map { "\"\($0)\"" } ?? "nil")
    }
}

struct Probe3PermissionsFixture: View {
    @StateObject private var asker = Probe3PermissionAsker()

    var body: some View {
        Form {
            Button("Ask location") { asker.askLocation() }
                .accessibilityIdentifier("probe3.perm.location")
            Button("Ask camera") { asker.askCamera() }
                .accessibilityIdentifier("probe3.perm.camera")
            Button("Ask microphone") { asker.askMicrophone() }
                .accessibilityIdentifier("probe3.perm.microphone")
            Button("Ask contacts") { asker.askContacts() }
                .accessibilityIdentifier("probe3.perm.contacts")
            Button("Ask photos") { asker.askPhotos() }
                .accessibilityIdentifier("probe3.perm.photos")
            Button("Ask tracking") { asker.askTracking() }
                .accessibilityIdentifier("probe3.perm.tracking")
            Button("Browse local network") { asker.browseLocalNetwork() }
                .accessibilityIdentifier("probe3.perm.localnet")
            Button("Read pasteboard") { asker.readPasteboard() }
                .accessibilityIdentifier("probe3.perm.paste")
            Text(asker.status)
                .accessibilityIdentifier("probe3.perm.status")
        }
        .navigationTitle("Permissions")
        .onDisappear { asker.stopLocalNetwork() }
    }
}

// MARK: - P12 / P3: badge and an incoming call

/// Reports an incoming CallKit call (ended by the app after 25 s whatever happens, so a call
/// never keeps ringing) and sets or clears the app-icon badge.
@MainActor
final class Probe3SystemActions: NSObject, ObservableObject, @preconcurrency CXProviderDelegate {
    @Published var status = "Idle"
    private var provider: CXProvider?
    private var callId: UUID?

    func reportIncomingCall() {
        let provider: CXProvider
        if let existing = self.provider {
            provider = existing
        } else {
            let configuration = CXProviderConfiguration()
            configuration.supportsVideo = false
            configuration.maximumCallsPerCallGroup = 1
            configuration.supportedHandleTypes = [.phoneNumber]
            provider = CXProvider(configuration: configuration)
            provider.setDelegate(self, queue: DispatchQueue.main)
            self.provider = provider
        }
        let id = UUID()
        callId = id
        let update = CXCallUpdate()
        update.remoteHandle = CXHandle(type: .phoneNumber, value: "5555550199")
        update.localizedCallerName = "Probe caller"
        update.hasVideo = false
        status = "Reporting call"
        Task { @MainActor in
            do {
                try await provider.reportNewIncomingCall(with: id, update: update)
                self.status = "Call reported (was: \(self.status))"
            } catch {
                let ns = error as NSError
                self.status = "Call report failed: \(ns.domain) \(ns.code) \(ns.localizedDescription) (was: \(self.status))"
            }
        }
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(25))
            if self.callId == id { self.endCall() }
        }
    }

    func endCall() {
        guard let provider, let id = callId else { return }
        provider.reportCall(with: id, endedAt: nil, reason: .remoteEnded)
        callId = nil
        status = "Call ended by the app"
    }

    @Published var taps = 0

    /// P14: blocks the main thread for 20 s, starting 1 s after the tap (so XCUITest's own
    /// tap, made through SpringBoard's coordinates, has returned): the front app hung.
    func hangSoon() {
        status = "Hang starts in 1 s"
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            MainActor.assumeIsolated { self?.hangNow() }
        }
    }

    private func hangNow() {
        status = "Hanging"
        Thread.sleep(forTimeInterval: 20)
        status = "Hang over (20 s)"
    }

    func setBadge(_ count: Int) {
        status = "Setting badge \(count)"
        Task { @MainActor in
            let center = UNUserNotificationCenter.current()
            do {
                let granted = try await center.requestAuthorization(options: [.badge])
                try await center.setBadgeCount(count)
                self.status = "badge \(count) set (badge authorization granted \(granted))"
            } catch {
                self.status = "badge failed: \(error.localizedDescription)"
            }
        }
    }

    /// Keeps what the status said before (run 139 showed only "Provider reset"): the order of
    /// CallKit's answers is the evidence.
    func providerDidReset(_ provider: CXProvider) {
        callId = nil
        status = "Provider reset (was: \(status))"
    }

    func provider(_ provider: CXProvider, perform action: CXAnswerCallAction) {
        action.fulfill()
        status = "Call answered"
    }

    func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
        action.fulfill()
        callId = nil
        status = "Call ended (end action)"
    }
}

struct Probe3SystemFixture: View {
    @StateObject private var actions = Probe3SystemActions()

    var body: some View {
        Form {
            Button("Set badge 7") { actions.setBadge(7) }
                .accessibilityIdentifier("probe3.system.badge")
            Button("Clear badge") { actions.setBadge(0) }
                .accessibilityIdentifier("probe3.system.clearbadge")
            Button("Report incoming call") { actions.reportIncomingCall() }
                .accessibilityIdentifier("probe3.system.call")
            Button("End call") { actions.endCall() }
                .accessibilityIdentifier("probe3.system.endcall")
            Button("Hang 20 s") { actions.hangSoon() }
                .accessibilityIdentifier("probe3.system.hang")
            Button("Count tap") { actions.taps += 1 }
                .accessibilityIdentifier("probe3.system.count")
            Text("Taps \(actions.taps)")
                .accessibilityIdentifier("probe3.system.taps")
            Text(actions.status)
                .accessibilityIdentifier("probe3.system.status")
        }
        .navigationTitle("System")
    }
}
