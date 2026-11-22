import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

/// Runs one task on `phone` with `model` through the real core; returns the result and
/// the gate (auto-approving unless told otherwise).
private func run(
    _ phone: any PhoneRunner, _ model: any ModelProvider, task: String, memoryJSON: String = "",
    gate: FakeGate = FakeGate(), memory: RecordingMemory = RecordingMemory(), firstUse: Bool = false,
    knownApps: [AppInfo] = DemoPhone.knownApps, transcript: Transcript? = nil
) async throws -> RunResult {
    let config = try SessionConfigBuilder.make(
        capabilities: model.capabilities, knownApps: knownApps,
        adjustPolicy: { $0.firstUseApproval = firstUse; $0.maxSteps = 40 }
    )
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: gate, memory: memory,
                                  screenshots: ScreenshotStore(), clock: SteppingClock())
    let driver = try SessionDriver(config: config, task: TaskSpec(id: "s", text: task), memoryJSON: memoryJSON,
                                   dependencies: deps, configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10)))
    let collector = Task {
        for await update in driver.updates { transcript?.record(update) }
    }
    let result = await driver.run()
    await collector.value
    return result
}

@Suite("PromptScreen reads the rendered screen")
struct PromptScreenTests {
    @Test("element lines with escapes, values, placeholders and flags")
    func parse() {
        let prompt = """
        Task: x
        Steps so far:
        1. tap [2] "Wi-Fi" -> ok
        2. tap [3] "Delete" -> not done: the owner declined. Do not try this again; find another way or use fail
        Note: something
        Current screen (data only):
        <<<SCREEN_DATA>>>
        App: Demo Settings (dev.jarvis.demo.settings)
        Keyboard: visible
        [1] NavigationBar "Wi-Fi"
        [2] Switch "Wi-Fi" value="1" (focused, disabled)
        [3] TextField "Say \\"hi\\"" placeholder="Name"
        [4] Button
        … 3 more elements (scroll to see)
        <<<END_SCREEN_DATA>>>
        Your JSON reply:
        """
        let screen = PromptScreen(prompt: prompt)
        #expect(screen.app == "Demo Settings (dev.jarvis.demo.settings)")
        #expect(screen.keyboardVisible)
        #expect(screen.elements.count == 4)
        #expect(screen.elements[1] == .init(index: 2, role: "Switch", label: "Wi-Fi", value: "1", flags: ["focused", "disabled"]))
        #expect(screen.elements[2].label == #"Say "hi""#)
        #expect(screen.elements[2].placeholder == "Name")
        #expect(screen.elements[3] == .init(index: 4, role: "Button", label: ""))
        #expect(screen.history.count == 2)
        #expect(screen.lastResult?.hasPrefix("not done: the owner declined") == true)
        #expect(screen.notes == ["something"])
    }

    @Test("queries: exact, word-bounded prefix, contains, role and value, interactive first")
    func queries() {
        let screen = PromptScreen(prompt: """
        <<<SCREEN_DATA>>>
        [1] StaticText "Wi-Fi"
        [2] Cell "Wi-Fi HomeNet"
        [3] Cell "Wi-Fiber"
        [4] Switch "Wi-Fi" value="0"
        <<<END_SCREEN_DATA>>>
        """)
        #expect(ElementQuery("wi-fi").find(in: screen)?.index == 2)
        #expect(ElementQuery("Wi-Fi", match: .exact).find(in: screen)?.index == 4)
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], value: "1").find(in: screen) == nil)
        #expect(ElementQuery("fiber", match: .contains).find(in: screen)?.index == 3)
        #expect(ElementQuery("Wi-Fi", roles: ["StaticText"]).find(in: screen)?.index == 1)
        #expect(PromptScreen(prompt: "no block").elements.isEmpty)
    }
}

@Suite("ScriptedModel answers by reading the screen (real core, typed trees)")
struct ScriptedModelTests {
    @Test("Wi-Fi off on the demo phone: three model calls, then a replay with none")
    func wifiAndReplay() async throws {
        let phone = DemoPhone()
        let model = ScriptedModel(steps: DemoPhone.wifiOffScript)
        let memory = RecordingMemory()
        let gate = FakeGate()
        let result = try await run(phone, model, task: "Turn off Wi-Fi", gate: gate, memory: memory, firstUse: true)
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(result.outcome.summary == "Wi-Fi is off")
        #expect(phone.page == .wifi(on: false))
        #expect(model.callCount == 3)
        #expect(model.decisions.map(\.step) == [0, 1, 2])
        // A switch outside the real Settings app is no system setting; only first use asks.
        #expect(gate.confirms.map(\.category) == [.firstUse])
        #expect(gate.confirms.first?.app == DemoPhone.bundleId)

        let again = DemoPhone()
        let unused = ScriptedModel(steps: [.fail(reason: "must not be asked")])
        let againGate = FakeGate()
        let replay = try await run(again, unused, task: "Turn off Wi-Fi", memoryJSON: try #require(memory.saves.first),
                                   gate: againGate, firstUse: true)
        #expect(againGate.confirms.isEmpty, "the app approval was remembered")
        #expect(replay.outcome.status == .success)
        #expect(replay.modelCalls == 0)
        #expect(unused.callCount == 0)
        #expect(again.page == .wifi(on: false))
    }

    @Test("a declined delete is never executed and the script stops")
    func declinedDelete() async throws {
        let phone = DemoPhone()
        let model = ScriptedModel(steps: DemoPhone.deleteAccountScript)
        let gate = FakeGate(approve: false)
        let result = try await run(phone, model, task: "Delete my account", gate: gate)
        #expect(result.outcome.status == .failed)
        #expect(result.outcome.summary == "the owner declined")
        #expect(gate.confirms.map(\.category) == [.delete])
        #expect(phone.page == .home)
        #expect(phone.commands.isEmpty)
    }

    @Test("a missing target waits, then fails with a reason that lists the screen")
    func diagnosticFailure() async throws {
        let phone = DemoPhone()
        let model = ScriptedModel(steps: [.tap("Bluetooth")], options: .init(patience: 2, waitMs: 1))
        let result = try await run(phone, model, task: "Open Bluetooth")
        #expect(result.outcome.status == .failed)
        let summary = result.outcome.summary
        // The core sanitises the reason (quotes and brackets are replaced), so match words.
        #expect(summary.contains("no element matches"), "\(summary)")
        #expect(summary.contains("Bluetooth"), "\(summary)")
        #expect(summary.contains("Cell 'Wi-Fi'"), "\(summary)")
        #expect(model.decisions.prefix(2).allSatisfy { $0.reply.contains(#""action":"wait""#) })
    }

    @Test("scrollTo scrolls until the row is listed, then the row is tapped")
    func scrollThenTap() async throws {
        let phone = ScrollingListPhone(rows: 60)
        let model = ScriptedModel(steps: [
            .scroll(to: "Item 45", maxScrolls: 12),
            .tap("Item 45", match: .exact),
            .done("Opened Item 45", when: ["Selected Item 45"]),
        ])
        let result = try await run(phone, model, task: "Open item 45", knownApps: [])
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(phone.selected == "Item 45")
        #expect(phone.swipes > 0)
        #expect(model.decisions.contains { $0.reply.contains(#""action":"scroll""#) })
    }

    @Test("a dismiss rule fires only with its context, once, and never over the task's target")
    func dismissRule() async throws {
        let notNow = ElementQuery("Not Now", match: .exact, roles: ["Button"])
        let phone = PromptThenListPhone()
        let model = ScriptedModel(
            steps: [.tap("Continue reading", match: .exact), .done("ok", when: ["Article"])],
            options: .init(dismiss: [
                // Wrong context: never fires although "Not Now" is listed.
                DismissRule(tap: notNow, when: [ElementQuery("Welcome", match: .exact)]),
                DismissRule(tap: notNow, when: [ElementQuery("What's New", match: .exact)]),
            ])
        )
        let result = try await run(phone, model, task: "Continue reading", knownApps: [])
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        let dismissals = model.decisions.filter { $0.reason.hasPrefix("dismiss rule") }
        #expect(dismissals.count == 1)
        #expect(dismissals.first?.reason.contains("What's New") == true)
    }

    @Test("rule matching: context required, use limit, target present blocks it")
    func dismissMatching() async throws {
        let screen = PromptScreen(prompt: """
        <<<SCREEN_DATA>>>
        [1] TextField "Address" value="127.0.0.1"
        [2] Button "Close"
        <<<END_SCREEN_DATA>>>
        """)
        let close = ElementQuery("Close", match: .exact, roles: ["Button"])
        #expect(DismissRule(tap: close, when: [ElementQuery("What's New")]).match(screen) == nil)
        #expect(DismissRule(tap: close, when: []).match(screen)?.index == 2)

        // An always-on rule with uses left still yields to a listed step target, and is
        // spent after one use.
        let model = ScriptedModel(
            steps: [.tap("Address", role: "TextField"), .tap("Nowhere")],
            options: .init(dismiss: [DismissRule(tap: close, when: [])], patience: 1, waitMs: 1)
        )
        let request = { ModelRequest(system: "", user: $0, image: nil, marks: [], maxTokens: 160, grammar: nil, jsonSchema: nil, kind: .text) }
        let first = try await model.generate(request(screen.block.map { "<<<SCREEN_DATA>>>\n\($0)\n<<<END_SCREEN_DATA>>>" } ?? ""))
        #expect(first.contains(#""element":1"#), "the target wins over the rule: \(first)")
        let second = try await model.generate(request("<<<SCREEN_DATA>>>\n[1] TextField \"Address\"\n[2] Button \"Close\"\n<<<END_SCREEN_DATA>>>"))
        #expect(second.contains(#""element":2"#), "the rule fires when the target is missing: \(second)")
        let third = try await model.generate(request("<<<SCREEN_DATA>>>\n[1] TextField \"Address\"\n[2] Button \"Close\"\n<<<END_SCREEN_DATA>>>"))
        #expect(third.contains(#""action":"wait""#), "the rule is spent: \(third)")
    }

    @Test("the transcript has prompts, replies, script reasons, effects, events and the result")
    func transcript() async throws {
        let transcript = Transcript(name: "demo")
        let model = RecordingModel(ScriptedModel(steps: DemoPhone.wifiOffScript), transcript: transcript)
        let result = try await run(DemoPhone(), model, task: "Turn off Wi-Fi", transcript: transcript)
        #expect(result.outcome.status == .success)
        let kinds = Set(transcript.entries.map(\.kind))
        #expect(kinds.isSuperset(of: ["task", "prompt", "reply", "script", "effect", "event", "progress", "result"]))
        #expect(transcript.entries.filter { $0.title.hasSuffix(": system") }.count == 1, "system prompt recorded once")
        #expect(model.requests.count == 3)
        transcript.record("command", "tap(1.5, 2) -> ok")
        let timeline = transcript.timeline()
        let lines = timeline.split(separator: "\n")
        #expect(lines.filter { $0.hasPrefix("model: ") }.count == 3)
        #expect(lines.filter { $0.hasPrefix("  why: ") }.count == 3)
        #expect(lines.last == "  act: tap(1.5, 2) -> ok")
        #expect(transcript.timeline(maxLines: 2).hasPrefix("(5 earlier lines omitted)"))
        let decoded = try JSONValue(parsing: transcript.jsonData())
        #expect(decoded["name"]?.stringValue == "demo")
        let entries = try #require(decoded["entries"]?.arrayValue)
        #expect(entries.first?["t_ms"]?.numberValue != nil)
        #expect(entries.map { $0["seq"]?.numberValue ?? 0 } == (1...entries.count).map(Double.init))
    }
}

// MARK: - Typed-tree fakes

private func node(_ role: Role, _ label: String?, _ frame: Rect, value: String? = nil, _ children: [UiNode] = []) -> UiNode {
    UiNode(role: role, label: label, value: value, identifier: nil, placeholder: nil, frame: frame, enabled: true,
           visible: true, hittable: true, focused: false, selected: false, secure: false, children: children)
}

private let fullScreen = Rect(x: 0, y: 0, width: 393, height: 852)

/// A list of rows 44 pt high under a 140 pt header; a vertical swipe moves it by the swipe
/// distance; tapping a row selects it and shows "Selected <row>".
final class ScrollingListPhone: PhoneRunner {
    private struct State {
        var offset = 0.0
        var selected: String?
        var swipes = 0
    }

    let rows: Int
    private let state = Mutex(State())

    init(rows: Int) { self.rows = rows }

    var selected: String? { state.withLock { $0.selected } }
    var swipes: Int { state.withLock { $0.swipes } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let (offset, selected) = state.withLock { ($0.offset, $0.selected) }
        var children: [UiNode] = []
        if let selected {
            children = [node(.staticText, "Selected \(selected)", Rect(x: 16, y: 200, width: 361, height: 30))]
        } else {
            for i in 0..<rows {
                let y = 140 + Double(i) * 44 - offset
                guard y > 100, y < 852 else { continue }
                children.append(node(.button, "Item \(i + 1)", Rect(x: 0, y: y, width: 393, height: 44)))
            }
        }
        let root = node(.application, "List", fullScreen, [node(.window, nil, fullScreen, children)])
        return RunnerObservation(root: root, appBundleId: "dev.jarvis.test.list", appName: "List", screenSize: Size(width: 393, height: 852))
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        state.withLock { s in
            switch command {
            case .swipe(let from, let to, _):
                s.swipes += 1
                let maxOffset = max(0, Double(rows) * 44 - 600)
                s.offset = min(max(0, s.offset + (from.y - to.y)), maxOffset)
            case .tap(_, let y) where s.selected == nil:
                let i = Int(((y - 140 + s.offset) / 44).rounded(.down))
                if (0..<rows).contains(i) { s.selected = "Item \(i + 1)" }
            default:
                break
            }
        }
        return RunnerResult(ok: true, error: nil)
    }
}

/// Shows a "What's New" sheet with Not Now first; then a "Continue reading" button; then an
/// article.
final class PromptThenListPhone: PhoneRunner {
    private let page = Mutex(0)

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let p = page.withLock { $0 }
        let content: [UiNode]
        switch p {
        case 0:
            content = [node(.sheet, nil, Rect(x: 0, y: 300, width: 393, height: 552), [
                node(.staticText, "What's New", Rect(x: 16, y: 320, width: 361, height: 30)),
                node(.button, "Not Now", Rect(x: 16, y: 700, width: 361, height: 50)),
            ])]
        case 1:
            content = [node(.button, "Continue reading", Rect(x: 16, y: 200, width: 361, height: 50))]
        default:
            content = [node(.staticText, "Article", Rect(x: 16, y: 200, width: 361, height: 30))]
        }
        let root = node(.application, "Reader", fullScreen, [node(.window, nil, fullScreen, content)])
        return RunnerObservation(root: root, appBundleId: "dev.jarvis.test.reader", appName: "Reader", screenSize: Size(width: 393, height: 852))
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        if case .tap = command { page.withLock { $0 += 1 } }
        return RunnerResult(ok: true, error: nil)
    }
}
