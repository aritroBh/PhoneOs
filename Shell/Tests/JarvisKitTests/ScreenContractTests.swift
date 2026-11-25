import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// What the Swift side (PromptScreen, ScriptedModel, LiveRun, the confirmation wording)
// depends on in the core's prompt. Every test here drives the real core and reads the prompt
// it really produced; nothing is a copied string. When the core changes how it renders a
// switch, a step result or a cue line, the failure names the contract, not a downstream
// script that "stopped finding" something.

/// Runs `task` on `phone` with `model` through the real core (first-use approval as given).
private func drive(
    _ phone: any PhoneRunner, _ model: FakeModel, task: String, gate: FakeGate = FakeGate(),
    firstUse: Bool = false, knownApps: [AppInfo] = DemoPhone.knownApps
) async throws -> RunResult {
    let config = try SessionConfigBuilder.make(
        capabilities: model.capabilities, knownApps: knownApps,
        adjustPolicy: { $0.firstUseApproval = firstUse; $0.maxSteps = 20 }
    )
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: gate, memory: RecordingMemory(),
                                  screenshots: ScreenshotStore(), clock: SteppingClock())
    let driver = try SessionDriver(config: config, task: TaskSpec(id: "c", text: task), memoryJSON: "",
                                   dependencies: deps, configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10)))
    let collector = Task { for await _ in driver.updates {} }
    let result = await driver.run()
    await collector.value
    return result
}

/// The demo phone with a scroll indicator and a page control added to every page, the way
/// iOS reports them (`Vertical scroll bar, 3 pages` valued `40%`, a `Page 1 of 2` value).
private final class CueingPhone: PhoneRunner {
    let inner = DemoPhone()

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let observation = try await inner.snapshot(includeScreenshot: includeScreenshot)
        let bar = DemoPhone.node(.other(name: "Other"), "Vertical scroll bar, 3 pages", value: "40%",
                                 Rect(x: 363, y: 140, width: 30, height: 712),
                                 [DemoPhone.node(.other(name: "Other"), nil, Rect(x: 387, y: 300, width: 3, height: 200))])
        let pager = DemoPhone.node(.other(name: "PageControl"), nil, value: "Page 1 of 2",
                                   Rect(x: 150, y: 800, width: 90, height: 20))
        guard var root = observation.root else { return observation }
        root.children[0].children += [bar, pager]
        return RunnerObservation(root: root, appBundleId: observation.appBundleId, appName: observation.appName,
                                 screenSize: observation.screenSize)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }
}

/// A home screen with one icon, `label`.
private final class HomeScreenPhone: PhoneRunner {
    let label: String
    let taps = Mutex(0)
    init(icon label: String) { self.label = label }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        let icon = DemoPhone.node(.button, label, Rect(x: 20, y: 100, width: 60, height: 60))
        let root = DemoPhone.node(.application, "Home Screen", full, [DemoPhone.node(.window, nil, full, [icon])])
        return RunnerObservation(root: root, appBundleId: "com.apple.springboard", appName: nil, screenSize: Size(width: 393, height: 852))
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        if case .tap = command { taps.withLock { $0 += 1 } }
        return RunnerResult(ok: true, error: nil)
    }
}

/// Probe run 134: the real edit menu's shape — raw `MenuItem`s in a collection view, no menu
/// or callout ancestor — copied through `UiTreeBuilder` as the runner does.
private final class EditMenuPhone: PhoneRunner {
    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let full = Rect(x: 0, y: 0, width: 393, height: 852)
        func item(_ label: String, _ x: Double, _ w: Double) -> RawElement {
            RawElement(elementType: XCUIElementTypeCode.menuItem, label: label, frame: Rect(x: x, y: 220, width: w, height: 44),
                       children: [RawElement(elementType: XCUIElementTypeCode.staticText, label: label,
                                             frame: Rect(x: x + 16, y: 233, width: w - 32, height: 18))])
        }
        let snapshot = RawElement(elementType: XCUIElementTypeCode.application, label: "Notes", frame: full, children: [
            RawElement(elementType: XCUIElementTypeCode.window, frame: full, children: [
                RawElement(elementType: XCUIElementTypeCode.staticText, label: "A note", frame: Rect(x: 20, y: 120, width: 200, height: 20)),
            ]),
            RawElement(elementType: XCUIElementTypeCode.window, frame: full, children: [
                RawElement(elementType: XCUIElementTypeCode.collectionView, frame: Rect(x: 20, y: 220, width: 312, height: 44), children: [
                    item("Copy", 24, 68), item("Replace…", 92, 99),
                ]),
            ]),
        ])
        let root = UiTreeBuilder.build(snapshot, screenSize: Size(width: 393, height: 852)).root
        return RunnerObservation(root: root, appBundleId: "com.apple.mobilenotes", appName: "Notes",
                                 screenSize: Size(width: 393, height: 852))
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { RunnerResult(ok: true, error: nil) }
}

@Suite("Contract with the core's prompt (real core, real prompts)", .timeLimit(.minutes(1)))
struct ScreenContractTests {
    private func tap(_ query: ElementQuery, in request: ModelRequest) -> String {
        guard let e = query.find(in: PromptScreen(prompt: request.user)) else {
            return #"{"thought":"nothing","action":"fail","reason":"not listed: \#(query)"}"#
        }
        return tapJSON(e.index)
    }

    @Test("a switch is rendered (on)/(off), PromptScreen reads it as a state, never as a value")
    func switchState() async throws {
        let wifi = ElementQuery("Wi-Fi", match: .exact, roles: ["Switch"])
        let model = FakeModel { request, call in
            switch call {
            case 1: return self.tap(ElementQuery("Wi-Fi", roles: ["Cell"]), in: request)
            case 2: return self.tap(wifi, in: request)
            default: return doneJSON
            }
        }
        let phone = DemoPhone()
        let result = try await drive(phone, model, task: "Turn off Wi-Fi")
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(phone.page == .wifi(on: false))

        let requests = model.requests
        #expect(requests.count == 3)
        // Call 2: the switch is on, in the form the core renders it.
        let before = PromptScreen(prompt: requests[1].user)
        let line = before.block?.split(separator: "\n").first { $0.contains("Switch") }.map(String.init)
        #expect(line?.hasSuffix("(on)") == true, "the core no longer renders a switch as `(on)`: \(line ?? "no Switch line")")
        #expect(line?.contains("value=") == false, "a switch line carries a value= again: \(line ?? "")")
        let sw = try #require(wifi.find(in: before))
        #expect(sw.isOn == true, "PromptScreen did not read the switch state of `\(sw.line)`")
        #expect(sw.value == nil)
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], isOn: true).find(in: before) != nil)
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], isOn: false).find(in: before) == nil)
        // Call 3: after the tap it reads off.
        let after = PromptScreen(prompt: requests[2].user)
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], isOn: false).find(in: after) != nil,
                "after the tap the switch should read off: \(after.block ?? "")")
        // The run view keeps showing the state.
        #expect(wifi.find(in: before)?.displayValue == "On")
        #expect(wifi.find(in: after)?.displayValue == "Off")
        // Anything else is not a switch.
        #expect(before.elements.filter { $0.role != "Switch" }.allSatisfy { $0.isOn == nil })
    }

    @Test("history: an ok step, a failed step and a not-done step are each classified")
    func historyOutcomes() async throws {
        // ok: the Wi-Fi row opens the page, and the result says what changed.
        let ok = FakeModel { request, call in
            call == 1 ? self.tap(ElementQuery("Wi-Fi", roles: ["Cell"]), in: request) : doneJSON
        }
        _ = try await drive(DemoPhone(), ok, task: "Open Wi-Fi")
        let okScreen = PromptScreen(prompt: ok.requests[1].user)
        #expect(okScreen.lastOutcome == .ok, "ok step read as \(String(describing: okScreen.lastOutcome)): \(okScreen.history)")
        #expect(okScreen.lastResult?.hasPrefix("ok;") == true, "an ok step lost its change summary: \(okScreen.history)")
        #expect(!ScriptedModel.needsRetry(result: okScreen.lastResult ?? "", query: ElementQuery("Wi-Fi", roles: ["Cell"]),
                                          on: okScreen, nextTargetListed: true))

        // failed: a tap that changes nothing on screen.
        let failed = FakeModel { request, call in
            call == 1 ? self.tap(ElementQuery("Settings", roles: ["Button"]), in: request) : doneJSON
        }
        _ = try await drive(HomeScreenPhone(icon: "Settings"), failed, task: "Tap the icon",
                            knownApps: [AppInfo(bundleId: "com.apple.Preferences", name: "Settings")])
        let failedScreen = PromptScreen(prompt: failed.requests[1].user)
        #expect(failedScreen.lastOutcome == .failed, "failed step read as \(String(describing: failedScreen.lastOutcome)): \(failedScreen.history)")
        #expect(ScriptedModel.needsRetry(result: failedScreen.lastResult ?? "", query: ElementQuery("Settings"),
                                         on: failedScreen, nextTargetListed: false))

        // not done: the owner declines the delete; `done` right after it is Blocked.
        let declined = FakeModel { request, call in
            call == 1 ? self.tap(ElementQuery("Delete account", roles: ["Button"]), in: request) : doneJSON
        }
        let gate = FakeGate(approve: false)
        let result = try await drive(DemoPhone(), declined, task: "Delete my account", gate: gate)
        let declinedScreen = PromptScreen(prompt: declined.requests[1].user)
        #expect(declinedScreen.lastOutcome == .notDone, "declined step read as \(String(describing: declinedScreen.lastOutcome)): \(declinedScreen.history)")
        #expect(declinedScreen.lastResult?.contains("owner declined") == true)
        #expect(result.outcome.status == .blocked, "done right after a decline should finish Blocked: \(result.outcome)")
    }

    @Test("cue lines (scroll, page) are read as cues and never as elements")
    func cueLines() async throws {
        let phone = CueingPhone()
        let model = FakeModel { _, _ in doneJSON }
        _ = try await drive(phone, model, task: "Look")
        let prompt = model.requests[0].user
        let screen = PromptScreen(prompt: prompt)
        #expect(screen.cues.moreBelow, "no `more below` cue in a prompt whose tree has a scroll bar at 40%: \(screen.block ?? "")")
        #expect(screen.cues.moreAbove)
        #expect(screen.cues.page?.number == 1 && screen.cues.page?.count == 2, "page cue not read: \(screen.block ?? "")")
        // The same screen without the extra nodes lists the same elements.
        let plain = FakeModel { _, _ in doneJSON }
        _ = try await drive(DemoPhone(), plain, task: "Look")
        let plainScreen = PromptScreen(prompt: plain.requests[0].user)
        #expect(screen.elements.map(\.line) == plainScreen.elements.map(\.line),
                "a cue line was parsed as an element, or the scroll bar was listed: \(screen.elements.map(\.line))")
        #expect(!screen.elements.isEmpty)
    }

    @Test("a tap on an unknown home-screen icon asks a first-use confirmation the sheet words as the icon")
    func homeScreenIcon() async throws {
        let phone = HomeScreenPhone(icon: "Weather")
        let model = FakeModel { request, call in
            call == 1 ? self.tap(ElementQuery("Weather", roles: ["Button"]), in: request) : doneJSON
        }
        let gate = FakeGate()
        _ = try await drive(phone, model, task: "Open Weather", gate: gate, firstUse: true, knownApps: [AppInfo(bundleId: "com.apple.Preferences", name: "Settings")])
        let confirm = try #require(gate.confirms.first, "the unknown icon was tapped without a question")
        #expect(confirm.category == .firstUse)
        #expect(confirm.app == ConfirmationWording.springboard)
        let wording = ConfirmationWording(confirm, appName: "SpringBoard")
        #expect(wording.title == #"Open "Weather" from the Home Screen?"#, "\(confirm.summary) -> \(wording.title)")
        #expect(wording.homeScreenTarget == #""Weather""#)
        #expect(!wording.title.contains("SpringBoard"))
        // An ordinary first-use request keeps its wording.
        let ordinary = ConfirmationWording(ConfirmRequest(category: .firstUse, summary: "Let Jarvis act in Mail (x)?", app: "com.apple.mobilemail", phoneOnly: false), appName: "Mail")
        #expect(ordinary.title == "Let Jarvis act in Mail?")
        #expect(ordinary.chip == riskCategoryTitle(.firstUse))
    }

    @Test("a tap on a known home-screen icon needs no question at the tap")
    func knownIcon() async throws {
        let phone = HomeScreenPhone(icon: "Settings")
        let model = FakeModel { request, call in
            call == 1 ? self.tap(ElementQuery("Settings", roles: ["Button"]), in: request) : doneJSON
        }
        let gate = FakeGate()
        _ = try await drive(phone, model, task: "Open Settings", gate: gate, firstUse: true, knownApps: [AppInfo(bundleId: "com.apple.Preferences", name: "Settings")])
        #expect(gate.confirms.isEmpty, "\(gate.confirms)")
        #expect(phone.taps.withLock { $0 } == 1)
    }

    @Test("probe run 134: a raw MenuItem reaches the core as a menu item; its hand-off asks")
    func menuItemThroughTheCore() async throws {
        let model = FakeModel { request, call in
            call == 1 ? self.tap(ElementQuery("Replace…", roles: ["MenuItem"]), in: request) : doneJSON
        }
        let gate = FakeGate(approve: false)
        _ = try await drive(EditMenuPhone(), model, task: "Fix my note", gate: gate)
        let prompt = model.requests[0].user
        #expect(prompt.contains(#"MenuItem "Replace…""#), "\(prompt)")
        let confirm = try #require(gate.confirms.first, "Replace… was tapped without a question")
        #expect(confirm.phoneOnly)
        #expect(confirm.summary.contains("may send the selected content to another app or service"), "\(confirm.summary)")
    }
}
