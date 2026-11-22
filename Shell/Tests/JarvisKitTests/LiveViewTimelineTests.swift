import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The Live view's timeline: the reducer over real and synthetic runs, its privacy rules,
// the driver's frame option, the encoding for the remote stream and the scene the view draws.

// MARK: - Builders

private let phone = Size(width: 393, height: 852)

private func node(
    _ role: Role, _ label: String? = nil, value: String? = nil, placeholder: String? = nil, _ frame: Rect,
    focused: Bool = false, secure: Bool = false, visible: Bool = true, _ children: [UiNode] = []
) -> UiNode {
    UiNode(role: role, label: label, value: value, identifier: nil, placeholder: placeholder, frame: frame, enabled: true,
           visible: visible, hittable: true, focused: focused, selected: false, secure: secure, children: children)
}

private let emailFrame = Rect(x: 16, y: 200, width: 361, height: 44)
private let passwordFrame = Rect(x: 16, y: 260, width: 361, height: 44)
private let sendFrame = Rect(x: 16, y: 400, width: 361, height: 44)
private let secretValue = "hunter2-SECRET"

/// A sign-in screen: Email (plain), Password (secure, holding a value), Send.
private func loginTree(focus: String? = nil, size: Size = phone) -> UiNode {
    let full = Rect(x: 0, y: 0, width: size.width, height: size.height)
    return node(.application, "Mail", full, [node(.window, nil, full, [
        node(.navigationBar, "Sign in", Rect(x: 0, y: 59, width: size.width, height: 81)),
        node(.textField, "Email", value: "me@example.org", placeholder: "you@example.com", emailFrame, focused: focus == "email"),
        node(.secureTextField, "Password", value: secretValue, placeholder: "Required", passwordFrame, focused: focus == "password",
             secure: true),
        // A secure-flagged text field (role TextField): its value is secret too.
        node(.textField, "PIN", value: "4321-PIN", Rect(x: 16, y: 320, width: 361, height: 44), focused: focus == "pin", secure: true),
        node(.button, "Send", sendFrame, [node(.staticText, "Send", Rect(x: 170, y: 410, width: 50, height: 20))]),
    ])])
}

private func observed(_ root: UiNode, size: Size = phone, locked: Bool = false, app: String? = "Mail") -> Event {
    .observed(observation: ScreenObservation(
        screen: Screen(appBundleId: "com.apple.mobilemail", appName: app, size: size, root: root, keyboardVisible: false,
                       orientation: size.width > size.height ? .landscapeLeft : .portrait),
        screenshot: nil, deviceLocked: locked
    ))
}

private func progress(_ step: UInt32, _ action: Action?) -> SessionProgress {
    SessionProgress(step: step, phase: .acting, lastAction: action, app: "Mail", modelCalls: 0, parseRetries: 0,
                    waitingForUnlock: false, lockedWaitMs: 0, lockedTotalMs: 0)
}

/// Builds the event stream a run produces, effect by effect.
private struct Run {
    var events: [RunEvent] = [.update(.started(TaskSpec(id: "task-1", text: "Sign in")))]
    var number = 0
    var coreStep: UInt32 = 0

    mutating func observe(_ event: Event, screenshot: Bool = false) {
        number += 1
        events.append(.update(.effect(.observe(screenshot: screenshot), number: number)))
        events.append(.update(.effectCompleted(.observe(screenshot: screenshot), event: event)))
    }

    mutating func prompt(_ text: String) {
        events.append(.prompt(text))
    }

    mutating func act(_ command: RunnerCommand, action: Action? = nil, result: RunnerResult = RunnerResult(ok: true, error: nil)) {
        number += 1
        coreStep += 1
        events.append(.update(.effect(.act(command: command), number: number)))
        events.append(.update(.progress(progress(coreStep, action))))
        events.append(.update(.effectCompleted(.act(command: command), event: .actResult(result: result))))
    }

    mutating func add(_ update: DriverUpdate) { events.append(.update(update)) }

    func reduce(maxSteps: Int = PointerTimeline.defaultMaxSteps, frames: Bool = false) -> PointerTimeline {
        var t = PointerTimeline(maxSteps: maxSteps, framesEnabled: frames)
        for e in events { t.apply(e) }
        return t
    }
}

private func json(_ timeline: ViewerTimeline) throws -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    return String(decoding: try encoder.encode(timeline), as: UTF8.self)
}

private let finishedOK = RunResult(outcome: Outcome(status: .success, summary: "ok", steps: 1, durationMs: 1),
                                   endReason: .sessionFinished, memory: .saved, effectCount: 1, modelCalls: 1)

// MARK: - The reducer

@Suite("Live view: pointer timeline", .timeLimit(.minutes(1)))
struct PointerTimelineTests {
    @Test("a demo run through the coordinator: one step per command the phone received, captions and points match")
    func demoRun() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-live-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero)
        let events = Mutex<[RunEvent]>([])
        let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: FakeGate()) { e in
            events.withLock { $0.append(e) }
        }
        guard case .finished(_, let result) = outcome else {
            Issue.record("not started: \(outcome)")
            return
        }
        #expect(result.outcome.status == .success)
        var timeline = PointerTimeline()
        for e in events.withLock({ $0 }) { timeline.apply(e) }
        // The commands the driver actually sent.
        let sent = events.withLock { $0 }.compactMap { e -> RunnerCommand? in
            if case .update(.effect(.act(let command), _)) = e { return command }
            return nil
        }
        #expect(sent.count == 2)
        let t = timeline.timeline
        #expect(t.steps.count == sent.count)
        for (step, command) in zip(t.steps, sent) {
            guard case .tap(let x, let y) = command else {
                Issue.record("demo sent \(command)")
                continue
            }
            #expect(step.gesture == .tap(ViewerPoint(x: x, y: y)))
            #expect(step.state == .done)
            #expect(t.screen(for: step)?.source == .tree)
            #expect(t.screen(for: step)?.appName == DemoPhone.appName)
            #expect(t.screen(for: step)?.frameKey == nil, "frames are off by default")
        }
        #expect(t.steps.map(\.caption) == ["Tap \u{201C}Wi-Fi\u{201D}", "Tap \u{201C}Wi-Fi\u{201D}"])
        #expect(t.steps[0].target?.role == "StaticText")
        #expect(t.steps[1].target?.role == "Switch")
        #expect(t.steps[1].target?.frame == ViewerRect(x: 312, y: 166, width: 51, height: 31))
        #expect(t.steps.map(\.number) == [1, 2])
        #expect(t.activity == .finished(.success))
        #expect(t.taskId == timeline.timeline.taskId && !t.taskId.isEmpty)
        // The wireframe of the first screen: the shown elements, without the app and window.
        let first = try #require(t.screen(for: t.steps[0]))
        #expect(first.elements.map(\.role) == ["NavigationBar", "Table", "Cell", "StaticText", "Button"])
        #expect(first.elements.map(\.depth) == [0, 0, 1, 2, 1])
        #expect(coordinator.viewerFrames.count == 0)
    }

    @Test("every gesture kind gets its gesture and caption")
    func everyGesture() {
        var run = Run()
        run.observe(observed(loginTree()))
        run.act(.tap(x: 100, y: 420))
        run.act(.longPress(x: 100, y: 220, durationMs: 900))
        run.act(.swipe(from: Point(x: 200, y: 700), to: Point(x: 200, y: 200), durationMs: 300))
        run.act(.swipe(from: Point(x: 200, y: 200), to: Point(x: 210, y: 700), durationMs: 300))
        run.act(.swipe(from: Point(x: 300, y: 500), to: Point(x: 20, y: 480), durationMs: 300))
        run.act(.swipe(from: Point(x: 20, y: 500), to: Point(x: 300, y: 520), durationMs: 300))
        run.act(.swipe(from: Point(x: 20, y: 500), to: Point(x: 20, y: 500), durationMs: 300))
        run.act(.pressKey(key: .return))
        run.act(.pressKey(key: .home))
        run.act(.launchApp(bundleId: "com.apple.Preferences"))
        run.act(.launchApp(bundleId: "com.example.unknown"))
        run.act(.openUrl(url: "https://user:pw@Example.com:8443/path/secret?q=token#frag"))
        run.act(.openUrl(url: "tel:+15551234567"))
        run.act(.wait(ms: 300))
        run.act(.wait(ms: 1_500))
        run.act(.wait(ms: 2_000))
        let t = run.reduce().timeline
        let pairs = t.steps.map { ($0.gesture, $0.caption) }
        let expected: [(ViewerGesture, String)] = [
            (.tap(ViewerPoint(x: 100, y: 420)), "Tap \u{201C}Send\u{201D}"),
            (.longPress(ViewerPoint(x: 100, y: 220), durationMs: 900), "Press and hold \u{201C}Email\u{201D}"),
            (.swipe(from: ViewerPoint(x: 200, y: 700), to: ViewerPoint(x: 200, y: 200), durationMs: 300), "Swipe up"),
            (.swipe(from: ViewerPoint(x: 200, y: 200), to: ViewerPoint(x: 210, y: 700), durationMs: 300), "Swipe down"),
            (.swipe(from: ViewerPoint(x: 300, y: 500), to: ViewerPoint(x: 20, y: 480), durationMs: 300), "Swipe left"),
            (.swipe(from: ViewerPoint(x: 20, y: 500), to: ViewerPoint(x: 300, y: 520), durationMs: 300), "Swipe right"),
            (.swipe(from: ViewerPoint(x: 20, y: 500), to: ViewerPoint(x: 20, y: 500), durationMs: 300), "Swipe"),
            (.key("Return"), "Press Return"),
            (.key("Home"), "Go home"),
            (.launch("Settings"), "Open Settings"),
            (.launch("com.example.unknown"), "Open com.example.unknown"),
            (.openURL(host: "example.com"), "Open example.com"),
            (.openURL(host: ""), "Open a link"),
            (.wait(ms: 300), "Wait 300 ms"),
            (.wait(ms: 1_500), "Wait 1.5 s"),
            (.wait(ms: 2_000), "Wait 2 s"),
        ]
        #expect(pairs.count == expected.count)
        for (got, want) in zip(pairs, expected) {
            #expect(got.0 == want.0)
            #expect(got.1 == want.1)
        }
        #expect(t.steps.allSatisfy { $0.state == .done })
        #expect(t.steps.allSatisfy { $0.screenID == t.currentScreenID })
        #expect(t.steps.map(\.coreStep) == (1...16).map { Optional($0) })
        let encoded = (try? json(t)) ?? ""
        #expect(!encoded.contains("secret") && !encoded.contains("token") && !encoded.contains("pw") && !encoded.contains("5551234567"))
    }

    @Test("the target is the deepest shown element under the point; none outside every element")
    func targets() {
        var run = Run()
        run.observe(observed(loginTree()))
        run.act(.tap(x: 180, y: 415)) // on the label inside the button
        run.act(.tap(x: 5, y: 600)) // on the window only
        let t = run.reduce().timeline
        #expect(t.steps[0].target?.role == "StaticText")
        #expect(t.steps[0].target?.depth == 1)
        #expect(t.steps[1].target == nil)
        #expect(t.steps[1].caption == "Tap")
    }

    @Test("a failed command is marked failed without the runner's words; a locked refusal says so")
    func failedAct() {
        var run = Run()
        run.observe(observed(loginTree(focus: "email")))
        run.act(.typeText(text: "abc"), action: .type(element: nil, text: "abc", submit: false),
                result: RunnerResult(ok: false, error: "could not type 'abc' into field"))
        run.act(.tap(x: 100, y: 420), result: .deviceLocked())
        run.add(.effect(.act(command: .tap(x: 1, y: 1)), number: 99))
        run.add(.finished(finishedOK))
        let t = run.reduce().timeline
        #expect(t.steps[0].state == .failed("The phone did not do it"))
        #expect(t.steps[1].state == .failed("The phone was locked"))
        #expect(t.steps[2].state == .failed("Not finished"), "a command still running when the run ends")
        #expect(!((try? json(t)) ?? "").contains("could not type"))
    }

    @Test("a lock mid-run keeps nothing of the lock screen, and the activity says to unlock")
    func lockMidRun() {
        var run = Run()
        run.observe(observed(loginTree()))
        run.act(.tap(x: 100, y: 420))
        run.add(.effect(.awaitUnlock(reason: "locked", waitedMs: 0), number: 50))
        run.add(.awaitingUnlock(reason: "locked", waitedMs: 0))
        var mid = PointerTimeline(framesEnabled: true)
        for e in run.events { mid.apply(e) }
        #expect(mid.timeline.activity == .locked)
        #expect(LiveViewScene.make(timeline: mid.timeline, replay: ReplayState(), now: 100).caption == "Unlock to continue")
        let lockRoot = node(.application, "SpringBoard", Rect(x: 0, y: 0, width: 393, height: 852),
                            [node(.staticText, "Enter passcode 1234", Rect(x: 10, y: 10, width: 200, height: 40))])
        run.observe(observed(lockRoot, locked: true))
        run.observe(observed(loginTree()))
        run.act(.tap(x: 100, y: 420))
        let t = run.reduce(frames: true).timeline
        let locked = t.screens.filter { $0.source == .locked }
        #expect(locked.isEmpty, "the lock screen is not kept once no step refers to it")
        var atLock = PointerTimeline(framesEnabled: true)
        for e in run.events.prefix(while: { e in
            if case .update(.effectCompleted(_, .observed(let o))) = e, o.deviceLocked { return false }
            return true
        }) { atLock.apply(e) }
        // The locked observation itself, applied alone.
        atLock.apply(.update(.effect(.observe(screenshot: false), number: 60)))
        atLock.apply(.update(.effectCompleted(.observe(screenshot: false), event: observed(lockRoot, locked: true))))
        let screen = atLock.timeline.currentScreen
        #expect(screen?.source == .locked)
        #expect(screen?.elements.isEmpty == true)
        #expect(screen?.frameKey == nil, "no frame for a locked observation")
        #expect(!((try? json(atLock.timeline)) ?? "").contains("passcode"))
        #expect(t.steps.count == 2 && t.steps.allSatisfy { $0.state == .done })
        #expect(t.steps[1].screenID != t.steps[0].screenID)
    }

    @Test("a runner-text screen has no wireframe: cursor and caption only")
    func runnerText() {
        var run = Run()
        run.observe(.observedRunnerText(snapshot: RunnerSnapshot(
            source: "Application 'Settings'\n  Button 'Wi-Fi' {{16, 200}, {361, 44}}", appBundleId: "com.apple.Preferences",
            appName: "Settings", size: phone, keyboardVisible: false, orientation: .portrait, screenshot: nil, deviceLocked: false
        )))
        run.act(.tap(x: 100, y: 220))
        run.act(.typeText(text: "hello"), action: .type(element: nil, text: "hello", submit: false))
        let t = run.reduce(frames: true).timeline
        let screen = t.currentScreen
        #expect(screen?.source == .runnerText)
        #expect(screen?.elements.isEmpty == true)
        #expect(screen?.frameKey == ViewerFrameKey(taskId: "task-1", observation: 1))
        #expect(t.steps[0].target == nil && t.steps[0].caption == "Tap")
        #expect(t.steps[1].caption == "Type text", "no tree, no prompt: the field cannot be checked, so the text is hidden")
    }

    @Test("points outside the screen, a zero-size screen and NaN are handled")
    func degenerate() throws {
        var run = Run()
        run.observe(observed(loginTree(), size: Size(width: 0, height: 0)))
        run.act(.tap(x: -50, y: 5_000))
        run.act(.tap(x: .nan, y: 3))
        run.act(.swipe(from: Point(x: .infinity, y: 0), to: Point(x: 0, y: .nan), durationMs: 10))
        run.act(.tap(x: 100, y: 420))
        let t = run.reduce().timeline
        #expect(t.steps.count == 4)
        #expect(t.steps[0].gesture == .tap(ViewerPoint(x: -50, y: 5_000)), "the command's point is kept as sent")
        #expect(t.steps[0].target == nil)
        #expect(t.steps[1].target == nil && t.steps[1].caption == "Tap")
        #expect(t.steps[2].caption == "Swipe")
        #expect(t.steps[3].caption == "Tap \u{201C}Send\u{201D}", "a zero-size screen uses the fallback phone's bounds")
        for step in t.steps {
            for ms in [0.0, 50, 500, 5_000] {
                let pose = try #require(CursorPath.pose(step: step.number, in: t, atMs: ms))
                let bounds = ViewerSize.fallbackPhone
                #expect(pose.position.isFinite)
                #expect(pose.position.x >= 0 && pose.position.x <= bounds.width && pose.position.y >= 0 && pose.position.y <= bounds.height)
            }
        }
    }

    @Test("landscape screens keep their size; targets and the mapping follow it")
    func landscape() throws {
        let size = Size(width: 852, height: 393)
        let root = node(.application, "Maps", Rect(x: 0, y: 0, width: 852, height: 393), [
            node(.button, "Directions", Rect(x: 700, y: 300, width: 120, height: 60)),
        ])
        var run = Run()
        run.observe(observed(root, size: size, app: "Maps"))
        run.act(.tap(x: 760, y: 330))
        let t = run.reduce().timeline
        let screen = try #require(t.currentScreen)
        #expect(screen.size == ViewerSize(width: 852, height: 393) && screen.size.isLandscape)
        #expect(t.steps[0].caption == "Tap \u{201C}Directions\u{201D}")
        let end = try #require(CursorPath.pose(step: 1, in: t, atMs: 1e9))
        #expect(end.position == ViewerPoint(x: 760, y: 330), "not clamped by a portrait size")
        let m = ViewerMapping(screen: screen.size, view: ViewerSize(width: 300, height: 600))
        #expect(m.contentRect.width == 300 && m.contentRect.height < 300)
    }

    @Test("1,000 commands keep the newest 200 steps and only the screens they need")
    func cap() {
        var run = Run()
        for i in 0..<1_000 {
            if i % 3 == 0 { run.observe(observed(loginTree())) }
            run.act(.tap(x: Double(i % 393), y: 420))
        }
        let t = run.reduce().timeline
        #expect(t.steps.count == 200)
        #expect(t.droppedSteps == 800)
        #expect(t.steps.first?.number == 801 && t.steps.last?.number == 1_000)
        #expect(t.stepNumbers == Array(801...1_000))
        #expect(t.screens.count <= 201)
        let used = Set(t.steps.compactMap(\.screenID))
        #expect(t.screens.allSatisfy { used.contains($0.id) || $0.id == t.currentScreenID })
        #expect(used.allSatisfy { id in t.screens.contains { $0.id == id } })
        #expect(t.step(number: 900)?.number == 900)
        #expect(t.step(number: 10) == nil)
        let small = run.reduce(maxSteps: 0).timeline
        #expect(small.steps.count == 1, "the cap is at least 1")
    }

    @Test("the wireframe: shown elements only, values never, capped at 400")
    func wireframe() throws {
        var children: [UiNode] = [
            node(.button, "Zero", Rect(x: 10, y: 10, width: 0, height: 20)),
            node(.button, "Off", Rect(x: -500, y: 10, width: 100, height: 20)),
            node(.button, "Hidden", Rect(x: 10, y: 10, width: 100, height: 20), visible: false),
            node(.button, "NaN", Rect(x: .nan, y: 10, width: 100, height: 20)),
            node(.button, "Half", Rect(x: -50, y: 10, width: 100, height: 20)),
            node(.secureTextField, nil, value: secretValue, placeholder: "Password", passwordFrame, secure: true),
            node(.other(name: "Pager"), "Page 1", Rect(x: 0, y: 800, width: 393, height: 20)),
        ]
        children += (0..<500).map { i in node(.cell, "Row \(i)", value: "value-\(i)", Rect(x: 0, y: Double(i), width: 393, height: 1)) }
        let root = node(.application, "App", Rect(x: 0, y: 0, width: 393, height: 852), [node(.window, nil, Rect(x: 0, y: 0, width: 393, height: 852), children)])
        var run = Run()
        run.observe(observed(root))
        let screen = try #require(run.reduce().timeline.currentScreen)
        #expect(screen.elements.count == PointerTimeline.maxElements)
        #expect(screen.elementsTruncated)
        let labels = screen.elements.compactMap(\.label)
        #expect(!labels.contains("Zero") && !labels.contains("Off") && !labels.contains("Hidden") && !labels.contains("NaN"))
        #expect(screen.elements.first?.label == "Half")
        #expect(screen.elements.first?.frame == ViewerRect(x: 0, y: 10, width: 50, height: 20), "clipped to the screen")
        let secure = try #require(screen.elements.first { $0.secure })
        #expect(secure.label == "Password" && secure.kind == .input)
        #expect(screen.elements.contains { $0.role == "Pager" && $0.kind == .container })
        #expect(!screen.elements.contains { $0.role == "Application" || $0.role == "Window" })
        let encoded = try json(run.reduce().timeline)
        #expect(!encoded.contains(secretValue))
        #expect(!encoded.contains("value-"), "element values are never kept")
    }

    @Test("a deep or huge tree does not stall the reducer")
    func pathologicalTree() {
        var deep = node(.button, "Leaf", Rect(x: 10, y: 10, width: 10, height: 10))
        for i in 0..<5_000 {
            deep = node(.other(name: "Group"), "G\(i)", Rect(x: 0, y: 0, width: 393, height: 852), [deep])
        }
        var run = Run()
        run.observe(observed(deep))
        run.act(.tap(x: 15, y: 15))
        let t = run.reduce().timeline
        #expect(t.currentScreen?.elements.count == PointerTimeline.maxElements)
        #expect(t.steps[0].target?.label == "Leaf")
    }

    @Test("what Jarvis is doing drives the caption while following")
    func activity() {
        var run = Run()
        run.observe(observed(loginTree()))
        var t = run.reduce()
        #expect(t.timeline.activity == .looking)
        t.apply(.update(.effect(.infer(request: ModelRequest(system: "", user: "", image: nil, marks: [], maxTokens: 1, grammar: nil, jsonSchema: nil,
                                                              kind: .text)), number: 9)))
        #expect(t.timeline.activity == .thinking)
        #expect(LiveViewScene.make(timeline: t.timeline, replay: ReplayState(), now: 0).caption == "Thinking…")
        t.apply(.update(.confirmationPending(ConfirmRequest(category: .delete, summary: "Delete the account", app: "Mail"))))
        #expect(t.timeline.activity == .waitingForApproval("Delete"))
        #expect(t.timeline.activity.caption == "Waiting for your OK · Delete")
        t.apply(.update(.questionPending("Which one?")))
        #expect(t.timeline.activity.caption == "Waiting for your answer")
        t.apply(.update(.finished(finishedOK)))
        #expect(t.timeline.activity == .finished(.success))
        let scene = LiveViewScene.make(timeline: t.timeline, replay: ReplayState(), now: 0)
        #expect(scene.caption == "Done" && scene.isFinished && !scene.isIdle)
    }

    @Test("a new run starts a fresh timeline with the same settings")
    func restart() {
        var run = Run()
        run.observe(observed(loginTree()))
        run.act(.tap(x: 100, y: 420))
        var t = run.reduce(maxSteps: 7, frames: true)
        t.apply(.update(.started(TaskSpec(id: "task-2", text: "Again"))))
        #expect(t.timeline.steps.isEmpty && t.timeline.screens.isEmpty && t.timeline.taskId == "task-2")
        #expect(t.maxSteps == 7 && t.framesEnabled)
    }
}

// MARK: - Typed text

@Suite("Live view: typed text is shown only when it is safe", .timeLimit(.minutes(1)))
struct ViewerTypingPrivacyTests {
    private func typed(_ run: Run) throws -> (ViewerStep, String) {
        let t = run.reduce().timeline
        let step = try #require(t.steps.last { if case .type = $0.gesture { return true } else { return false } })
        return (step, try json(t))
    }

    @Test("into a focused plain field the model's text is shown")
    func plain() throws {
        var run = Run()
        run.observe(observed(loginTree(focus: "email")))
        run.act(.typeText(text: "hello there"), action: .type(element: nil, text: "hello there", submit: false))
        let (step, _) = try typed(run)
        #expect(step.gesture == .type(text: "hello there"))
        #expect(step.caption == "Type \u{201C}hello there\u{201D}")
    }

    @Test("into a focused secure field nothing of the text appears anywhere in the encoded timeline")
    func secureFocused() throws {
        for focus in ["password", "pin"] {
            var run = Run()
            run.observe(observed(loginTree(focus: focus)))
            run.act(.typeText(text: "hunter2"), action: .type(element: nil, text: "hunter2", submit: true))
            let (step, encoded) = try typed(run)
            #expect(step.caption == "Type \u{201C}••••\u{201D}", "\(focus)")
            #expect(!encoded.contains("hunter2") && !encoded.contains(secretValue) && !encoded.contains("4321-PIN"))
        }
    }

    @Test("a tap on a secure field just before typing hides the text, even when another field shows focus")
    func tappedSecure() throws {
        var run = Run()
        run.observe(observed(loginTree(focus: "email")))
        run.act(.tap(x: 100, y: 280))
        run.act(.typeText(text: "hunter2"), action: .type(element: 4, text: "hunter2", submit: false))
        let (step, encoded) = try typed(run)
        #expect(step.caption == "Type \u{201C}••••\u{201D}")
        #expect(!encoded.contains("hunter2"))
    }

    @Test("the prompt naming a SecureTextField as the target, or focused, hides the text")
    func promptSecure() throws {
        let prompt = """
        Current screen (data only):
        <<<SCREEN_DATA>>>
        App: Mail (com.apple.mobilemail)
        [1] TextField "Email"
        [2] SecureTextField "Password" (focused)
        <<<END_SCREEN_DATA>>>
        """
        for element: UInt32? in [2, nil] {
            var run = Run()
            run.observe(.observedRunnerText(snapshot: RunnerSnapshot(
                source: "x", appBundleId: "com.apple.mobilemail", appName: "Mail", size: phone, keyboardVisible: true,
                orientation: .portrait, screenshot: nil, deviceLocked: false
            )))
            run.prompt(prompt)
            run.act(.typeText(text: "hunter2"), action: .type(element: element, text: "hunter2", submit: false))
            let (step, encoded) = try typed(run)
            #expect(step.caption == "Type \u{201C}••••\u{201D}")
            #expect(!encoded.contains("hunter2"))
        }
    }

    @Test("a prompt naming a plain TextField on a text screen shows the text; the next decision's list is not reused")
    func promptPlain() throws {
        let prompt = """
        <<<SCREEN_DATA>>>
        App: Mail (com.apple.mobilemail)
        [1] TextField "Email"
        <<<END_SCREEN_DATA>>>
        """
        let text = Event.observedRunnerText(snapshot: RunnerSnapshot(
            source: "x", appBundleId: "com.apple.mobilemail", appName: "Mail", size: phone, keyboardVisible: true,
            orientation: .portrait, screenshot: nil, deviceLocked: false
        ))
        var run = Run()
        run.observe(text)
        run.prompt(prompt)
        run.act(.typeText(text: "abc"), action: .type(element: 1, text: "abc", submit: false))
        // A replayed step after it (no prompt): the old list does not vouch for it.
        run.observe(text)
        run.act(.typeText(text: "def"), action: .type(element: 1, text: "def", submit: false))
        let t = run.reduce().timeline
        #expect(t.steps[0].caption == "Type \u{201C}abc\u{201D}")
        #expect(t.steps[1].caption == "Type text")
    }

    @Test("stored values typed through placeholders are masked; the runner's text never shows")
    func placeholders() throws {
        var run = Run()
        run.observe(observed(loginTree(focus: "email")))
        run.act(.typeText(text: "jane.doe@private.example"),
                action: .type(element: nil, text: "{{profile.email}}", submit: false))
        run.act(.typeText(text: "Hi jane.doe@private.example!"),
                action: .type(element: nil, text: "Hi {{ profile.email }}!", submit: false))
        let t = run.reduce().timeline
        #expect(t.steps[0].caption == "Type \u{201C}••••\u{201D}")
        #expect(t.steps[1].caption == "Type \u{201C}Hi ••••!\u{201D}")
        #expect(!(try json(t)).contains("private.example"))
    }

    @Test("with no evidence about the field, or a model text that is not what was typed, nothing is shown")
    func doubt() throws {
        var noFocus = Run()
        noFocus.observe(observed(loginTree()))
        noFocus.act(.typeText(text: "abc"), action: .type(element: nil, text: "abc", submit: false))
        #expect(try typed(noFocus).0.caption == "Type text")

        var mismatch = Run()
        mismatch.observe(observed(loginTree(focus: "email")))
        mismatch.act(.typeText(text: "something else"), action: .type(element: nil, text: "abc", submit: false))
        let (step, encoded) = try typed(mismatch)
        #expect(step.caption == "Type text")
        #expect(!encoded.contains("something else") && !encoded.contains("\"abc\""))

        var noProgress = Run()
        noProgress.observe(observed(loginTree(focus: "email")))
        noProgress.act(.typeText(text: "abc"), action: .tap(element: 3))
        #expect(try typed(noProgress).0.caption == "Type text")

        var noScreen = Run()
        noScreen.act(.typeText(text: "abc"), action: .type(element: nil, text: "abc", submit: false))
        #expect(try typed(noScreen).0.caption == "Type text")
    }

    @Test("long and multi-line text is flattened and capped")
    func capped() throws {
        let long = String(repeating: "a", count: 100) + "\nline"
        var run = Run()
        run.observe(observed(loginTree(focus: "email")))
        run.act(.typeText(text: long), action: .type(element: nil, text: long, submit: false))
        let (step, _) = try typed(run)
        guard case .type(let shown?) = step.gesture else {
            Issue.record("no text")
            return
        }
        #expect(shown.count == ViewerPrivacy.maxTypedCharacters && shown.hasSuffix("…") && !shown.contains("\n"))
        #expect(ViewerPrivacy.typedText("  \n ", secure: false) == nil)
        #expect(ViewerPrivacy.typedText("anything", secure: true) == ViewerPrivacy.mask)
        #expect(ViewerPrivacy.maskPlaceholders("a {{x}} b {{y") == "a •••• b ••••")
        #expect(ViewerPrivacy.host(of: "notaurl") == nil)
        #expect(ViewerPrivacy.host(of: "HTTPS://Sub.Example.COM/a?b") == "sub.example.com")
    }
}

// MARK: - The driver's frame option

/// A phone that counts screenshot requests and returns pixels when asked.
private final class CountingPhone: PhoneRunner {
    let inner: any PhoneRunner
    private let requests = Mutex<[Bool]>([])

    init(_ inner: any PhoneRunner) { self.inner = inner }

    var screenshotRequests: [Bool] { requests.withLock { $0 } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        requests.withLock { $0.append(includeScreenshot) }
        var observation = try await inner.snapshot(includeScreenshot: includeScreenshot)
        if includeScreenshot {
            observation.screenshot = CapturedScreenshot(data: Data(repeating: 1, count: 64), mimeType: "image/jpeg",
                                                        pixelWidth: 393, pixelHeight: 852)
        }
        return observation
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }
    func lockState() async throws -> DeviceLockState { try await inner.lockState() }
}

private struct FrameRun {
    var updates: [DriverUpdate]
    var result: RunResult
}

private func driveDemo(
    _ phone: CountingPhone, frames: ViewerFrameStore?, option: Bool, vision: Bool = false, screenshots: ScreenshotStore = ScreenshotStore()
) async throws -> FrameRun {
    // With vision, the Delete-account task ends on a screen with nothing to tap, so the core
    // asks for a screenshot there.
    let model = ScriptedModel(steps: vision ? DemoPhone.deleteAccountScript : DemoPhone.wifiOffScript,
                              capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: vision))
    let config = try SessionConfigBuilder.make(capabilities: model.capabilities, knownApps: DemoPhone.knownApps,
                                               adjustPolicy: { $0.firstUseApproval = false; $0.maxSteps = 40 })
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(), memory: RecordingMemory(),
                                  screenshots: screenshots, clock: SteppingClock(), viewerFrames: frames)
    let driver = try SessionDriver(
        config: config, task: TaskSpec(id: "frames", text: vision ? "Delete my account" : "Turn off Wi-Fi"), memoryJSON: "", dependencies: deps,
        configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10), unlockPollInterval: .milliseconds(5),
                                           lockWatchInterval: nil, viewerScreenshots: option)
    )
    let collector = Task { () -> [DriverUpdate] in
        var all: [DriverUpdate] = []
        for await update in driver.updates { all.append(update) }
        return all
    }
    let result = await driver.run()
    return FrameRun(updates: await collector.value, result: result)
}

/// The core's observe requests and what it got back, with the effect number.
private func observations(_ updates: [DriverUpdate]) -> [(number: Int, asked: Bool, gotRef: Bool, locked: Bool)] {
    var out: [(Int, Bool, Bool, Bool)] = []
    var number = 0
    for u in updates {
        if case .effect(_, let n) = u { number = n }
        guard case .effectCompleted(let effect, let event) = u else { continue }
        let asked: Bool
        switch effect {
        case .observe(let screenshot): asked = screenshot
        case .awaitUnlock: asked = false
        default: continue
        }
        switch event {
        case .observed(let o): out.append((number, asked, o.screenshot != nil, o.deviceLocked))
        case .observedRunnerText(let s): out.append((number, asked, s.screenshot != nil, s.deviceLocked))
        default: break
        }
    }
    return out
}

@Suite("Live view: the driver's real-screen option", .timeLimit(.minutes(1)))
struct ViewerFrameDriverTests {
    @Test("off: the runner is asked for pixels exactly when the core asks, and no frame is kept")
    func offByDefault() async throws {
        #expect(!DriverConfiguration().viewerScreenshots)
        for vision in [false, true] {
            let phone = CountingPhone(DemoPhone())
            let frames = ViewerFrameStore()
            let run = try await driveDemo(phone, frames: frames, option: false, vision: vision)
            #expect(run.result.outcome.status == .success)
            let asked = observations(run.updates).map(\.asked)
            #expect(phone.screenshotRequests == asked, "vision \(vision)")
            #expect(frames.count == 0)
        }
    }

    @Test("on: every observation is asked for pixels, but the core gets an ImageRef only when it asked")
    func onKeepsCoreUnchanged() async throws {
        for vision in [false, true] {
            let phone = CountingPhone(DemoPhone())
            let frames = ViewerFrameStore()
            let screenshots = ScreenshotStore()
            let run = try await driveDemo(phone, frames: frames, option: true, vision: vision, screenshots: screenshots)
            #expect(run.result.outcome.status == .success)
            let seen = observations(run.updates)
            #expect(!seen.isEmpty)
            if vision { #expect(seen.contains { $0.asked }, "the core asked for a screenshot of the thin demo screen") }
            #expect(phone.screenshotRequests.allSatisfy { $0 })
            for o in seen {
                #expect(o.gotRef == o.asked, "observation #\(o.number): the core asked \(o.asked)")
            }
            #expect(screenshots.storedIDs.count == seen.filter(\.asked).count, "the model's store only holds what the core asked for")
            #expect(Set(frames.keys) == Set(seen.map { ViewerFrameKey(taskId: "frames", observation: $0.number) }))
            // The reducer points each screen at its frame.
            var timeline = PointerTimeline(framesEnabled: true)
            for u in run.updates { timeline.apply(.update(u)) }
            for step in timeline.timeline.steps {
                let key = try #require(timeline.timeline.screen(for: step)?.frameKey)
                #expect(frames.frame(for: key) != nil)
            }
        }
    }

    @Test("on, without a frame store: nothing changes")
    func onWithoutStore() async throws {
        let phone = CountingPhone(DemoPhone())
        let run = try await driveDemo(phone, frames: nil, option: true)
        #expect(phone.screenshotRequests == observations(run.updates).map(\.asked))
    }

    @Test("on, with the phone locked: no frame is kept for a locked observation")
    func lockedKeepsNothing() async throws {
        let phone = CountingPhone(DemoPhone(lockedPolls: 3))
        let frames = ViewerFrameStore()
        let run = try await driveDemo(phone, frames: frames, option: true)
        #expect(run.result.outcome.status == .success)
        let seen = observations(run.updates)
        let locked = seen.filter(\.locked)
        #expect(!locked.isEmpty, "the run saw the lock screen")
        for o in locked {
            #expect(frames.frame(for: ViewerFrameKey(taskId: "frames", observation: o.number)) == nil)
        }
        #expect(frames.count == seen.count - locked.count)
    }

    @Test("the coordinator passes the setting through and starts each run with an empty store")
    func coordinator() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("jarvis-live-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let frames = ViewerFrameStore()
        frames.put(CapturedScreenshot(data: Data([1, 2, 3]), mimeType: "image/png", pixelWidth: 1, pixelHeight: 1),
                   for: ViewerFrameKey(taskId: "old", observation: 1))
        let coordinator = JarvisCoordinator(paths: JarvisPaths(directory: dir), selection: .demo, demoDelay: .zero, viewerFrames: frames)
        var settings = JarvisSettings()
        settings.liveViewScreenshots = true
        let outcome = await coordinator.run(task: "Turn off Wi-Fi", settings: settings, human: FakeGate()) { _ in }
        guard case .finished = outcome else {
            Issue.record("not started")
            return
        }
        // The demo phone takes no screenshots, so the old frame is gone and none was added.
        #expect(frames.count == 0)
        #expect(frames.isEnabled)
        _ = await coordinator.run(task: "Turn off Wi-Fi", settings: JarvisSettings(), human: FakeGate()) { _ in }
        #expect(!frames.isEnabled, "with the option off the store accepts nothing")
    }
}

// MARK: - Encoding and the scene

@Suite("Live view: encoding and scene", .timeLimit(.minutes(1)))
struct ViewerEncodingTests {
    @Test("the timeline survives an encode/decode round trip with stable keys")
    func roundTrip() throws {
        var run = Run()
        run.observe(observed(loginTree(focus: "email")))
        run.act(.tap(x: 100, y: 420))
        run.act(.longPress(x: 100, y: 220, durationMs: 900))
        run.act(.swipe(from: Point(x: 1, y: 2), to: Point(x: 3, y: 4), durationMs: 5))
        run.act(.typeText(text: "hi"), action: .type(element: nil, text: "hi", submit: false))
        run.act(.typeText(text: "x"))
        run.act(.pressKey(key: .tab), result: RunnerResult(ok: false, error: "nope"))
        run.act(.launchApp(bundleId: "com.apple.mobilesafari"))
        run.act(.openUrl(url: "https://example.com/a"))
        run.act(.wait(ms: 10))
        run.add(.effect(.callTool(call: ToolCall(name: "clock_now", argsJson: "{}")), number: 77))
        var t = run.reduce(frames: true).timeline
        t.screens.append(ViewerScreen(id: 999, size: ViewerSize(width: 1, height: 2), appName: nil, bundleId: "", source: .locked,
                                      elements: [], elementsTruncated: true, frameKey: nil, focusedInputSecure: true))
        let data = try JSONEncoder().encode(t)
        let back = try JSONDecoder().decode(ViewerTimeline.self, from: data)
        #expect(back == t)
        let text = try json(t)
        for key in [#""kind":"tap""#, #""kind":"long_press""#, #""duration_ms":900"#, #""kind":"swipe""#, #""kind":"type""#,
                    #""kind":"key""#, #""kind":"launch""#, #""kind":"open_url""#, #""host":"example.com""#, #""kind":"wait""#,
                    #""screen_id":"#, #""core_step":"#, #""task_id":"task-1""#, #""frame_key":{"observation":1,"task_id":"task-1"}"#,
                    #""kind":"failed""#, #""kind":"using_tool""#, #""source":"tree""#, #""source":"locked""#,
                    #""elements_truncated":true"#, #""current_screen_id":"#, #""dropped_steps":0"#] {
            #expect(text.contains(key), "missing \(key)")
        }
        for activity: ViewerActivity in [.idle, .looking, .thinking, .acting, .usingTool("t"), .waitingForApproval("Delete"),
                                         .waitingForAnswer, .locked, .finished(.interrupted)] {
            let decoded = try JSONDecoder().decode(ViewerActivity.self, from: try JSONEncoder().encode(activity))
            #expect(decoded == activity)
        }
        #expect(throws: (any Error).self) {
            try JSONDecoder().decode(ViewerGesture.self, from: Data(#"{"kind":"teleport"}"#.utf8))
        }
        let kind = try JSONDecoder().decode([ViewerElementKind].self, from: Data(#"["control","hologram"]"#.utf8))
        #expect(kind == [.control, .container], "an unknown kind from a newer sender reads as a container")
    }

    @Test("live: the newest step plays out, then the cursor idles on the latest screen")
    func liveScene() throws {
        var run = Run()
        run.observe(observed(loginTree()))
        run.act(.tap(x: 100, y: 420))
        run.observe(observed(loginTree()))
        var pt = run.reduce()
        pt.apply(.update(.effect(.infer(request: ModelRequest(system: "", user: "", image: nil, marks: [], maxTokens: 1, grammar: nil, jsonSchema: nil,
                                                               kind: .text)), number: 40)))
        let t = pt.timeline
        var replay = ReplayState()
        replay.sync(numbers: t.stepNumbers, now: 100)
        let playing = LiveViewScene.make(timeline: t, replay: replay, now: 100.1)
        #expect(playing.step?.number == 1 && playing.caption == "Tap \u{201C}Send\u{201D}" && !playing.isIdle)
        #expect(playing.screen?.id == t.steps[0].screenID)
        let later = LiveViewScene.make(timeline: t, replay: replay, now: 200)
        #expect(later.step == nil && later.caption == "Thinking…" && later.isIdle)
        #expect(later.screen?.id == t.currentScreenID)
        #expect(later.pose.position == ViewerPoint(x: 100, y: 420))
        #expect(later.position == nil && later.accessibilityValue == "Thinking…")
    }

    @Test("replay: the scrubbed step on the screen it acted on, with its position")
    func replayScene() throws {
        var run = Run()
        run.observe(observed(loginTree()))
        run.act(.tap(x: 100, y: 420))
        run.observe(observed(loginTree(focus: "email")))
        run.act(.tap(x: 100, y: 220), result: RunnerResult(ok: false, error: "x"))
        let t = run.reduce().timeline
        var replay = ReplayState()
        replay.sync(numbers: t.stepNumbers, now: 0)
        replay.stepBack(numbers: t.stepNumbers, now: 1)
        let back = LiveViewScene.make(timeline: t, replay: replay, now: 1)
        #expect(back.step?.number == 1 && back.position == "Step 1 of 2" && back.screen?.id == t.steps[0].screenID)
        #expect(back.accessibilityValue == "Tap \u{201C}Send\u{201D}, Step 1 of 2")
        replay.stepForward(numbers: t.stepNumbers, now: 2)
        let forward = LiveViewScene.make(timeline: t, replay: replay, now: 2)
        #expect(forward.caption == "Tap \u{201C}Email\u{201D}")
        #expect(forward.accessibilityValue == "Tap \u{201C}Email\u{201D}, did not work, Step 2 of 2")
        let empty = LiveViewScene.make(timeline: ViewerTimeline(), replay: ReplayState(), now: 0)
        #expect(empty.caption == "Standing by" && empty.screen == nil && empty.step == nil)
    }
}
