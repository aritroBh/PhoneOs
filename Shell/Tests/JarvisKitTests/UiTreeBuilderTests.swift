import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

private typealias T = XCUIElementTypeCode

private func raw(_ type: UInt, _ label: String = "", value: String? = nil, id: String = "", placeholder: String? = nil,
                 _ frame: Rect, enabled: Bool = true, selected: Bool = false, focus: Bool? = nil,
                 _ children: [RawElement] = []) -> RawElement {
    RawElement(elementType: type, label: label, value: value, identifier: id, placeholder: placeholder,
               frame: frame, isEnabled: enabled, isSelected: selected, hasFocus: focus, children: children)
}

private func raw(_ type: UInt, _ frame: Rect, _ children: [RawElement] = []) -> RawElement {
    RawElement(elementType: type, frame: frame, children: children)
}

private let full = Rect(x: 0, y: 0, width: 393, height: 852)

/// A login screen as an XCUITest snapshot of the playground would copy it.
private let loginSnapshot = raw(T.application, "Jarvis", full, [
    raw(T.window, full, [
        raw(T.navigationBar, "Login form", id: "Login form", Rect(x: 0, y: 59, width: 393, height: 96), [
            raw(T.button, "Playground", Rect(x: 8, y: 62, width: 100, height: 44)),
            raw(T.staticText, "Login form", Rect(x: 140, y: 70, width: 113, height: 22)),
        ]),
        raw(T.textField, "Username", value: "Username", id: "login.username", placeholder: "Username",
            Rect(x: 16, y: 180, width: 361, height: 44)),
        raw(T.secureTextField, "Password", value: "••••••••", id: "login.password", placeholder: "Password",
            Rect(x: 16, y: 240, width: 361, height: 44), focus: true),
        raw(T.button, "Sign In", id: "login.submit", Rect(x: 16, y: 300, width: 361, height: 50), enabled: false),
        raw(T.switch, "Show tips", value: "1", Rect(x: 312, y: 380, width: 51, height: 31)),
        raw(T.staticText, "Off screen", Rect(x: 16, y: 1_200, width: 200, height: 20)),
        raw(T.other, Rect(x: 0, y: 0, width: 0, height: 0)),
        raw(T.keyboard, Rect(x: 0, y: 560, width: 393, height: 292), [
            raw(T.key, "q", Rect(x: 3, y: 570, width: 36, height: 42)),
        ]),
        raw(999, "Mystery", Rect(x: 16, y: 420, width: 100, height: 20)),
    ]),
])

@Suite("UiTreeBuilder: XCUITest snapshot copies to the core's UiNode")
struct UiTreeBuilderTests {
    @Test("roles, values, flags and visibility")
    func conversion() throws {
        let result = UiTreeBuilder.build(loginSnapshot, screenSize: Size(width: 393, height: 852))
        #expect(result.nodeCount == loginSnapshot.count)
        #expect(result.keyboardVisible)
        #expect(result.unmappedTypes == ["Other": 1, "Type999": 1])

        let window = try #require(result.root.children.first)
        #expect(result.root.role == .application)
        #expect(result.root.label == "Jarvis")
        let nodes = window.children
        #expect(nodes[0].role == .navigationBar)
        #expect(nodes[0].identifier == "Login form")

        let username = nodes[1]
        #expect(username.role == .textField)
        #expect(username.value == nil, "XCUITest reports the placeholder as the value of an empty field")
        #expect(username.placeholder == "Username")

        let password = nodes[2]
        #expect(password.role == .secureTextField)
        #expect(password.secure)
        #expect(password.value == nil, "secure values never leave the runner")
        #expect(password.focused)

        let signIn = nodes[3]
        #expect(signIn.role == .button)
        #expect(!signIn.enabled)
        #expect(signIn.identifier == "login.submit")

        #expect(nodes[4].role == .switch)
        #expect(nodes[4].value == "1")
        #expect(!nodes[5].visible, "below the screen")
        #expect(!nodes[6].visible, "zero area")
        #expect(nodes[7].role == .keyboard)
        #expect(nodes[8].role == .other(name: "Type999"))
        #expect(nodes.allSatisfy { $0.hittable == $0.visible })
    }

    @Test("every XCTest type maps; icons and menu items are pressable; containers stay containers")
    func roleTable() {
        #expect(T.role(T.icon) == .button)
        #expect(T.role(T.toggle) == .switch)
        #expect(T.role(T.cell) == .cell)
        #expect(T.role(T.webView) == .webView)
        #expect(T.role(T.alert) == .alert)
        #expect(T.role(T.segmentedControl) == .other(name: "SegmentedControl"))
        #expect(T.role(T.pickerWheel) == .picker)
        #expect(T.role(T.searchField) == .searchField)
        for raw in UInt(0)...82 {
            #expect(!T.name(raw).hasPrefix("Type"), "type \(raw) has a name")
        }
    }

    @Test("label falls back to title, empty strings become nil, unknown screen keeps every frame visible")
    func fallbacks() {
        let element = RawElement(elementType: T.button, title: "Done", frame: Rect(x: 0, y: 2_000, width: 10, height: 10))
        let result = UiTreeBuilder.build(element, screenSize: nil)
        #expect(result.root.label == "Done")
        #expect(result.root.identifier == nil)
        #expect(result.root.visible)
        #expect(!result.keyboardVisible)
    }

    @Test("subtree focus: found, readable but none, unreadable")
    func subtreeFocus() {
        #expect(loginSnapshot.subtreeFocus == true)
        let none = raw(T.window, full, [raw(T.textField, "A", Rect(x: 0, y: 0, width: 10, height: 10), focus: false)])
        #expect(none.subtreeFocus == false)
        #expect(raw(T.window, full).subtreeFocus == nil)
    }

    @Test("dumps are readable and carry the raw type numbers")
    func dumps() {
        let result = UiTreeBuilder.build(loginSnapshot, screenSize: Size(width: 393, height: 852))
        let typed = UiTreeBuilder.dump(result.root)
        #expect(typed.contains(#"    SecureTextField "Password" placeholder="Password" id=login.password {{16, 240}, {361, 44}} [focused, secure]"#))
        #expect(typed.contains(#"Button "Sign In" id=login.submit {{16, 300}, {361, 50}} [disabled]"#))
        #expect(typed.contains("[hidden]"))
        #expect(!typed.contains("••"))
        let rawDump = UiTreeBuilder.dump(loginSnapshot)
        #expect(rawDump.hasPrefix("Application(2) label=\"Jarvis\""))
        #expect(rawDump.contains("SecureTextField(50)"))
        #expect(rawDump.contains("[focus]"))
        #expect(rawDump.contains("focus=?"))
    }

    @Test("a converted tree drives the real core through Event.observed")
    func typedTreeThroughCore() async throws {
        let built = UiTreeBuilder.build(loginSnapshot, screenSize: Size(width: 393, height: 852))
        let phone = StaticTreePhone(root: built.root, keyboard: built.keyboardVisible)
        let model = FakeModel { request, _ in
            // The typed tree keeps "disabled", which runner text cannot carry.
            #expect(request.user.contains(#"Button "Sign In" (disabled)"#))
            #expect(request.user.contains(#"SecureTextField "Password" (focused)"#))
            #expect(request.user.contains("Keyboard: visible"))
            #expect(!request.user.contains("••"))
            return #"{"thought":"seen","action":"done","summary":"read the screen"}"#
        }
        var h = Harness()
        h.model = model
        let config = try SessionConfigBuilder.make(capabilities: model.capabilities)
        let driver = try SessionDriver(
            config: config, task: TaskSpec(id: "typed", text: "Read the login screen"), memoryJSON: "",
            dependencies: DriverDependencies(runner: phone, model: model, tools: h.tools, human: h.gate, memory: h.memory, screenshots: h.screenshots)
        )
        let result = await driver.run()
        #expect(result.outcome.status == .success)
        #expect(model.callCount == 1)
    }
}

/// A runner that always shows one typed tree.
final class StaticTreePhone: PhoneRunner {
    let root: UiNode
    let keyboard: Bool

    init(root: UiNode, keyboard: Bool = false) {
        self.root = root
        self.keyboard = keyboard
    }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        RunnerObservation(root: root, appBundleId: "dev.jarvis.app", appName: "Jarvis",
                          screenSize: Size(width: 393, height: 852), keyboardVisible: keyboard)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        RunnerResult(ok: true, error: nil)
    }
}
