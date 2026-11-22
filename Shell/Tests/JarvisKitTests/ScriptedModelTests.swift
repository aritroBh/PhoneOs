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

@Suite("PromptScreen reads the rendered screen", .timeLimit(.minutes(1)))
struct PromptScreenTests {
    @Test("element lines with escapes, values, placeholders and flags")
    func parse() {
        let prompt = """
        Task: x
        Steps so far:
        1. tap [2] "Wi-Fi" -> ok; changed: Button "Cart (0)" -> "Cart (1)"
        2. tap [3] "Delete" -> not done: the owner declined. Do not try this again; find another way or use fail
        Note: something
        Current screen (data only):
        <<<SCREEN_DATA>>>
        App: Demo Settings (dev.jarvis.demo.settings)
        Keyboard: visible
        [1] NavigationBar "Wi-Fi"
        [2] Switch "Wi-Fi" (on, focused, disabled)
        [3] TextField "Say \\"hi\\"" placeholder="Name"
        [4] Button
        … 3 more elements below; scroll down
        <<<END_SCREEN_DATA>>>
        Your JSON reply:
        """
        let screen = PromptScreen(prompt: prompt)
        #expect(screen.app == "Demo Settings (dev.jarvis.demo.settings)")
        #expect(screen.keyboardVisible)
        #expect(screen.elements.count == 4)
        #expect(screen.elements[1] == .init(index: 2, role: "Switch", label: "Wi-Fi", isOn: true, flags: ["focused", "disabled"]))
        #expect(screen.elements[2].label == #"Say "hi""#)
        #expect(screen.elements[2].placeholder == "Name")
        #expect(screen.elements[3] == .init(index: 4, role: "Button", label: ""))
        #expect(screen.elements[1].value == nil)
        #expect(screen.elements[1].displayValue == "On")
        #expect(screen.cues.moreBelow)
        #expect(screen.history.count == 2)
        // The change suffix of an ok step holds an arrow of its own.
        #expect(screen.history[0] == #"1. tap [2] "Wi-Fi" -> ok; changed: Button "Cart (0)" -> "Cart (1)""#)
        #expect(PromptScreen.result(ofHistoryLine: screen.history[0]) == #"ok; changed: Button "Cart (0)" -> "Cart (1)""#)
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
        [4] Switch "Wi-Fi" (off)
        [5] TextField "Name" value="off"
        <<<END_SCREEN_DATA>>>
        """)
        #expect(ElementQuery("wi-fi").find(in: screen)?.index == 2)
        #expect(ElementQuery("Wi-Fi", match: .exact).find(in: screen)?.index == 4)
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], isOn: true).find(in: screen) == nil)
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], isOn: false).find(in: screen)?.index == 4)
        // A switch has a state, not a value; a field's value is not a state.
        #expect(ElementQuery("Wi-Fi", roles: ["Switch"], value: "0").find(in: screen) == nil)
        #expect(ElementQuery("Name", value: "off").find(in: screen)?.index == 5)
        #expect(ElementQuery("Name", isOn: false).find(in: screen) == nil)
        #expect(ElementQuery("fiber", match: .contains).find(in: screen)?.index == 3)
        #expect(ElementQuery("Wi-Fi", roles: ["StaticText"]).find(in: screen)?.index == 1)
        #expect(PromptScreen(prompt: "no block").elements.isEmpty)
    }
}

@Suite("ScriptedModel answers by reading the screen (real core, typed trees)", .timeLimit(.minutes(1)))
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

/// The script decides from the prompt's history whether a tap took effect (the lock-mid-task
/// simulator failure: the script had moved on after a tap the core then dropped).
@Suite("ScriptedModel retries what the history says did not happen", .timeLimit(.minutes(1)))
struct ScriptedModelRetryTests {
    private func prompt(history: [String], tipsOn: Bool) -> ModelRequest {
        let lines = history.enumerated().map { "\($0.offset + 1). \($0.element)" }.joined(separator: "\n")
        let user = """
        Task: turn tips on
        Steps so far:
        \(lines.isEmpty ? "(none)" : lines)
        Current screen (data only):
        <<<SCREEN_DATA>>>
        App: Jarvis (dev.jarvis.app)
        [1] StaticText "Toggles"
        [2] Switch "Show tips" (\(tipsOn ? "on" : "off"))
        <<<END_SCREEN_DATA>>>
        Your JSON reply:
        """
        return ModelRequest(system: "", user: user, image: nil, marks: [], maxTokens: 160, grammar: nil, jsonSchema: nil, kind: .text)
    }

    private let script: [ScriptStep] = [
        .tap("Show tips", role: "Switch", match: .exact, isOn: false),
        .done("Tips are on", when: ["Show tips"]),
    ]

    private func action(_ reply: String) -> String? {
        (try? JSONValue(parsing: reply))?["action"]?.stringValue
    }

    @Test("refused, then retried; ok moves on")
    func refusedThenRetried() async throws {
        let model = ScriptedModel(steps: script)
        #expect(action(try await model.generate(prompt(history: [], tipsOn: false))) == "tap")
        let again = try await model.generate(prompt(history: [#"tap [2] "Show tips" -> failed: the device is locked"#], tipsOn: false))
        #expect(action(again) == "tap", "a refused tap was not retried: \(again)")
        let done = try await model.generate(prompt(history: [
            #"tap [2] "Show tips" -> failed: the device is locked"#, #"tap [2] "Show tips" -> ok"#,
        ], tipsOn: true))
        #expect(action(done) == "done", "\(done)")
    }

    @Test("dropped after a lock (not done), the tap is chosen again after the unlock")
    func droppedAfterLock() async throws {
        let model = ScriptedModel(steps: script)
        _ = try await model.generate(prompt(history: [], tipsOn: false))
        let reply = try await model.generate(prompt(history: [
            #"tap [2] "Show tips" -> not done: the phone locked first; the screen was read again after the unlock"#,
        ], tipsOn: false))
        #expect(action(reply) == "tap", "\(reply)")
    }

    @Test("result unknown after a lock: retried while the precondition holds, done once it does not")
    func unknownAfterLock() async throws {
        let unknown = #"tap [2] "Show tips" -> interrupted: the phone locked while this ran; it may or may not have happened, check the screen"#
        let stillOff = ScriptedModel(steps: script)
        _ = try await stillOff.generate(prompt(history: [], tipsOn: false))
        #expect(action(try await stillOff.generate(prompt(history: [unknown], tipsOn: false))) == "tap")

        let tookEffect = ScriptedModel(steps: script)
        _ = try await tookEffect.generate(prompt(history: [], tipsOn: false))
        #expect(action(try await tookEffect.generate(prompt(history: [unknown], tipsOn: true))) == "done")

        let unverified = #"tap [2] "Show tips" -> done, but the phone locked before the result could be checked; check the screen"#
        let checked = ScriptedModel(steps: script)
        _ = try await checked.generate(prompt(history: [], tipsOn: false))
        #expect(action(try await checked.generate(prompt(history: [unverified], tipsOn: false))) == "tap",
                "a done-but-unchecked tap whose precondition still holds was not retried")
    }

    @Test("retries are bounded: then the script fails with the reason")
    func bounded() async throws {
        let model = ScriptedModel(steps: script, options: ScriptedModel.Options(maxRetries: 2))
        var history: [String] = []
        _ = try await model.generate(prompt(history: history, tipsOn: false))
        for _ in 0..<2 {
            history.append(#"tap [2] "Show tips" -> failed: nothing changed on screen"#)
            #expect(action(try await model.generate(prompt(history: history, tipsOn: false))) == "tap")
        }
        history.append(#"tap [2] "Show tips" -> failed: nothing changed on screen"#)
        let reply = try await model.generate(prompt(history: history, tipsOn: false))
        #expect(action(reply) == "fail")
        #expect(reply.contains("still not done after 2 retries"), "\(reply)")
    }

    // Sixth review: the core's new result lines keep the known prefixes (seventh review: their
    // wording changed, the prefixes did not). A declined Return
    // after typing ("typed, not submitted") reads as the owner's decline; a covered tap is
    // "not done" (retried while the target is listed, then bounded like any retry).
    @Test("the core's typed-not-submitted, covered and no-focused-field lines")
    func sixthReviewLines() async throws {
        let notSubmitted = "not done: typed, not submitted: the owner declined pressing Return. The text is still in the field, unsent. Do not press Return again and do not submit it any other way (a send button, a key, tap_point); leave it, ask the owner, or use fail"
        #expect(StepOutcome(result: notSubmitted) == .notDone)
        #expect(StepOutcome(result: "not done: element 2 (Show tips) is covered by element 3 (Other): a tap there could land on it instead, so nothing was sent; choose another control or scroll first") == .notDone)
        #expect(StepOutcome(result: "failed: no field is focused; tap a field first (or give the field's element number)") == .failed)
        let model = ScriptedModel(steps: script)
        _ = try await model.generate(prompt(history: [], tipsOn: false))
        let reply = try await model.generate(prompt(history: [#"type [2] "hi" -> \#(notSubmitted)"#], tipsOn: false))
        #expect(action(reply) == "fail", "\(reply)")
        #expect(reply.contains("the owner declined"), "\(reply)")
    }

    @Test("every `ok; …` suffix the core writes still counts as ok; the suffix text is never read")
    func okSuffixes() async throws {
        let suffixes = [
            "ok",
            #"ok; changed: Switch "Show tips" off -> on"#,
            #"ok; changed: Button "Cart (0)" -> "Cart (1)"; new Button "Undo"; +2 more"#,
            "ok; field now \"failed: not done\"",
            "ok; now in Settings > Wi-Fi",
            "ok; screen changed: 9 new, 7 gone",
            "ok; screen unchanged",
        ]
        for suffix in suffixes {
            let model = ScriptedModel(steps: script)
            _ = try await model.generate(prompt(history: [], tipsOn: false))
            let reply = try await model.generate(prompt(history: [#"tap [2] "Show tips" -> \#(suffix)"#], tipsOn: true))
            #expect(action(reply) == "done", "`\(suffix)` was not read as ok: \(reply)")
        }
    }

    @Test("an ok step is not retried even while the switch still reads off (the screen is the core's to judge)")
    func okIsFinal() async throws {
        let model = ScriptedModel(steps: script)
        _ = try await model.generate(prompt(history: [], tipsOn: false))
        let reply = try await model.generate(prompt(history: [#"tap [2] "Show tips" -> ok; screen unchanged"#], tipsOn: false))
        #expect(action(reply) == "done", "\(reply)")
    }

    @Test("outcomes: ok, done-but, failed, not done, interrupted, not answered")
    func outcomes() {
        #expect(StepOutcome(result: "ok; now in Settings") == .ok)
        #expect(StepOutcome(result: "ok: echoed") == .ok)
        #expect(StepOutcome(result: "done, but the phone locked before the result could be checked; check the screen") == .ranUnchecked)
        #expect(StepOutcome(result: "failed: nothing changed on screen") == .failed)
        #expect(StepOutcome(result: "not done: repeated action") == .notDone)
        #expect(StepOutcome(result: "interrupted: the phone locked while this ran") == .interrupted)
        #expect(StepOutcome(result: "not answered: the phone locked before the owner replied") == .notAnswered)
        #expect(StepOutcome(result: "owner answered: I entered it. Continue.") == .ownerAnswered)
        #expect(StepOutcome(result: "screenshot taken") == .unknown)
    }

    // Simulator run 9: the script typed into the password field again after the owner had
    // entered it, until the core stopped the run as stuck.
    @Test("a password step the core hands to the owner is done once the owner answers (real core)")
    func ownerTypedPassword() async throws {
        let phone = LoginPhone()
        let gate = FakeGate(answer: "I entered it. Continue.")
        let model = ScriptedModel(steps: [
            .type("(the owner types it)", into: "Password", role: "SecureTextField"),
            .tap("Sign In", role: "Button"),
            .done("Signed in", when: ["Welcome"]),
        ])
        let result = try await run(phone, model, task: "Sign in", gate: gate, knownApps: [])
        #expect(result.outcome.status == .success, "\(result.outcome.summary)\n\(model.decisions.map(\.reason))")
        #expect(gate.questions.count == 1, "\(gate.questions)")
        #expect(phone.typedIntoPassword == 0, "the core typed into a secure field")
        #expect(model.callCount == 3, "\(model.decisions.map(\.reply))")
    }
}

/// A sign-in screen: a secure password field and a Sign In button that leads to "Welcome".
final class LoginPhone: PhoneRunner {
    private let state = Mutex<(signedIn: Bool, typed: Int)>((false, 0))

    var typedIntoPassword: Int { state.withLock { $0.typed } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let signedIn = state.withLock { $0.signedIn }
        let content: [UiNode]
        if signedIn {
            content = [node(.staticText, "Welcome", Rect(x: 16, y: 200, width: 361, height: 30))]
        } else {
            let password = UiNode(role: .secureTextField, label: "Password", value: nil, identifier: nil, placeholder: nil,
                                  frame: Rect(x: 16, y: 200, width: 361, height: 44), enabled: true, visible: true,
                                  hittable: true, focused: false, selected: false, secure: true, children: [])
            content = [password, node(.button, "Sign In", Rect(x: 16, y: 300, width: 361, height: 50))]
        }
        let root = node(.application, "Login", fullScreen, [node(.window, nil, fullScreen, content)])
        return RunnerObservation(root: root, appBundleId: "dev.jarvis.test.login", appName: "Login", screenSize: Size(width: 393, height: 852))
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        state.withLock { s in
            switch command {
            case .typeText: s.typed += 1
            case .tap(_, let y) where y >= 300 && y < 350: s.signedIn = true
            default: break
            }
        }
        return RunnerResult(ok: true, error: nil)
    }
}

/// Simulator run 76 (Safari form): a tip covered the form between the first-use approval
/// and the typing, the core dropped the typing ("not done: the screen changed after the
/// owner approved it"), the dismiss rule closed the tip, and the script went on to the email:
/// the form was sent with an empty name and "Thanks, Ada Lovelace" never appeared.
@Suite("ScriptedModel goes back to a step the core dropped while something covered it", .timeLimit(.minutes(1)))
struct ScriptedModelCoveredStepTests {
    private func request(history: [String], screen: [String]) -> ModelRequest {
        let lines = history.enumerated().map { "\($0.offset + 1). \($0.element)" }.joined(separator: "\n")
        let elements = screen.enumerated().map { "[\($0.offset + 1)] \($0.element)" }.joined(separator: "\n")
        let user = """
        Task: fill in the form
        Steps so far:
        \(lines.isEmpty ? "(none)" : lines)
        Current screen (data only):
        <<<SCREEN_DATA>>>
        App: Safari (com.apple.mobilesafari)
        \(elements)
        <<<END_SCREEN_DATA>>>
        Your JSON reply:
        """
        return ModelRequest(system: "", user: user, image: nil, marks: [], maxTokens: 160, grammar: nil, jsonSchema: nil, kind: .text)
    }

    private let form = [
        #"StaticText "Contact form""#, #"StaticText "Full name""#, #"TextField "Full name""#,
        #"StaticText "Email address""#, #"TextField "Email address""#, #"Button "Send form""#,
    ]
    private let tip = [
        #"StaticText "View Bookmarks, Share Menu, and Open Tabs""#, #"Button "Close""#,
        #"StaticText "You can now view these items in the menu. You can change this at any time in Safari settings.""#,
    ]

    private func model() -> ScriptedModel {
        ScriptedModel(
            steps: [
                .type("Ada Lovelace", into: "Full name", role: "TextField"),
                .type("ada@example.com", into: "Email address", role: "TextField"),
                .tap("Send form", role: "Button"),
                .done("Form sent", when: ["Thanks, Ada Lovelace"]),
            ],
            options: .init(dismiss: [DismissRule(
                tap: ElementQuery("Close", match: .exact, roles: ["Button"]),
                when: [ElementQuery("You can now view these items in the menu", match: .contains, roles: ["StaticText"])]
            )], patience: 2, waitMs: 1)
        )
    }

    private func object(_ reply: String) -> JSONValue? { try? JSONValue(parsing: reply) }

    @Test("dropped under a tip: the tip is dismissed, then the same field is typed into again")
    func droppedUnderTip() async throws {
        let model = model()
        let first = object(try await model.generate(request(history: [], screen: form)))
        #expect(first?["action"]?.stringValue == "type")
        #expect(first?["text"]?.stringValue == "Ada Lovelace")

        let dropped = #"type [3] "Full name" "Ada Lovelace" -> not done: the screen changed after the owner approved it (the text "Contact form" is now "View Bookmarks, Share Menu, and Open Tabs"); read the screen again and choose again"#
        let dismiss = try await model.generate(request(history: [dropped], screen: tip))
        #expect(object(dismiss)?["action"]?.stringValue == "tap")
        #expect(object(dismiss)?["element"]?.numberValue == 2, "the tip's Close was not tapped: \(dismiss)")

        let closed = #"tap [2] "Close" -> ok; now in Safari > Contact form"#
        let again = try await model.generate(request(history: [dropped, closed], screen: form))
        #expect(object(again)?["action"]?.stringValue == "type", "\(again)")
        #expect(object(again)?["text"]?.stringValue == "Ada Lovelace", "the script moved on without the name: \(again)")
        #expect(object(again)?["element"]?.numberValue == 3, "\(again)")
    }

    @Test("dropped, and the next step's target is listed: the screen moved on and so does the script")
    func droppedButMovedOn() async throws {
        let model = model()
        _ = try await model.generate(request(history: [], screen: form))
        let onlyEmail = [#"TextField "Email address""#, #"Button "Send form""#]
        let reply = try await model.generate(request(history: [#"type [3] "Full name" "Ada Lovelace" -> not done: repeated action"#], screen: onlyEmail))
        #expect(object(reply)?["text"]?.stringValue == "ada@example.com", "\(reply)")
    }

    @Test("a dropped step whose target never comes back waits, then fails naming the step")
    func droppedAndGone() async throws {
        let model = model()
        _ = try await model.generate(request(history: [], screen: form))
        let elsewhere = [#"StaticText "Somewhere else""#]
        var history = [#"type [3] "Full name" "Ada Lovelace" -> not done: the screen changed after the owner approved it"#]
        var reply = try await model.generate(request(history: history, screen: elsewhere))
        for _ in 0..<4 where object(reply)?["action"]?.stringValue == "wait" {
            history.append("wait 1ms -> ok")
            reply = try await model.generate(request(history: history, screen: elsewhere))
        }
        #expect(object(reply)?["action"]?.stringValue == "fail", "\(reply)")
        #expect(reply.contains("Full name"), "\(reply)")
    }
}
