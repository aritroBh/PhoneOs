import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// `LiveViewModel`, the shell's adapter over the core's `JarvisViewer`: one FFI feed per
// update, a bounded wireframe cache, every FFI error absorbed, and a real demo run through
// the coordinator end to end. The viewer's own logic is tested in core/jarvis-viewer.

/// A `JarvisViewerProtocol` that forwards to a real viewer, counts the calls, and throws
/// from the methods named in `failing` (the test seam of `LiveViewModel.init(viewer:)`).
private final class CountingViewer: JarvisViewerProtocol {
    private let inner: JarvisViewer
    private let counts = Mutex<[String: Int]>([:])
    private let failingMethods = Mutex<Set<String>>([])

    init(framesEnabled: Bool = false, maxSteps: UInt32 = LiveViewModel.maxSteps) throws {
        inner = try JarvisViewer(config: CoreViewerConfig(maxSteps: maxSteps, framesEnabled: framesEnabled,
                                                          knownApps: LiveViewModel.knownApps))
    }

    func calls(_ method: String) -> Int { counts.withLock { $0[method] ?? 0 } }
    func fail(_ methods: Set<String>) { failingMethods.withLock { $0 = methods } }

    private func call<T>(_ method: String, _ body: () throws -> T) throws -> T {
        counts.withLock { $0[method, default: 0] += 1 }
        if failingMethods.withLock({ $0.contains(method) }) { throw JarvisError.Internal(message: "injected \(method)") }
        return try body()
    }

    func encode() throws -> Data { try call("encode") { try inner.encode() } }
    func feed(update: CoreViewerUpdate) throws -> CoreViewerChange { try call("feed") { try inner.feed(update: update) } }
    func generation() throws -> UInt64 { try call("generation") { try inner.generation() } }
    func replay(action: CoreReplayAction, nowMs: Double, reduceMotion: Bool) throws -> CoreReplayStatus {
        try call("replay") { try inner.replay(action: action, nowMs: nowMs, reduceMotion: reduceMotion) }
    }
    func replayStatus() throws -> CoreReplayStatus { try call("replayStatus") { try inner.replayStatus() } }
    func scene(query: CoreSceneQuery) throws -> CoreLiveScene { try call("scene") { try inner.scene(query: query) } }
    func screen(id: UInt32) throws -> CoreViewerScreen? { try call("screen") { try inner.screen(id: id) } }
    func timeline() throws -> CoreViewerTimeline { try call("timeline") { try inner.timeline() } }
    func approvalTarget() throws -> CoreApprovalTarget? { try call("approvalTarget") { try inner.approvalTarget() } }
    func feedAndStream(update: CoreViewerUpdate, hub: JarvisRemoteHub, nowMs: UInt64) throws -> CoreViewerFed {
        try call("feedAndStream") { try inner.feedAndStream(update: update, hub: hub, nowMs: nowMs) }
    }
    func applyDelta(bytes: Data) throws -> CoreDeltaApplied { try call("applyDelta") { try inner.applyDelta(bytes: bytes) } }
    func needsResync() throws -> Bool { try call("needsResync") { try inner.needsResync() } }
}

private let phone = Size(width: 393, height: 852)

private func button(_ label: String, y: Double) -> UiNode {
    UiNode(role: .button, label: label, value: nil, identifier: nil, placeholder: nil,
           frame: Rect(x: 16, y: y, width: 361, height: 44), enabled: true, visible: true, hittable: true, focused: false,
           selected: false, secure: false, children: [])
}

private func observed(_ label: String) -> Event {
    let full = Rect(x: 0, y: 0, width: phone.width, height: phone.height)
    let root = UiNode(role: .application, label: "Mail", value: nil, identifier: nil, placeholder: nil, frame: full, enabled: true,
                      visible: true, hittable: true, focused: false, selected: false, secure: false,
                      children: [button(label, y: 400)])
    return .observed(observation: ScreenObservation(
        screen: Screen(appBundleId: "com.apple.mobilemail", appName: "Mail", size: phone, root: root, keyboardVisible: false,
                       orientation: .portrait),
        screenshot: nil, deviceLocked: false
    ))
}

/// The events of a run of `taps` observe + tap pairs, each on a screen of its own.
private func tapRun(_ taps: Int, taskId: String = "many") -> [RunEvent] {
    var events: [RunEvent] = [.update(.started(TaskSpec(id: taskId, text: "Tap")))]
    var number = 0
    for i in 0..<taps {
        number += 1
        events.append(.update(.effect(.observe(screenshot: false), number: number)))
        events.append(.update(.effectCompleted(.observe(screenshot: false), event: observed("Send \(i)"))))
        number += 1
        let command = RunnerCommand.tap(x: 100, y: 420)
        events.append(.update(.effect(.act(command: command), number: number)))
        events.append(.update(.effectCompleted(.act(command: command), event: .actResult(result: RunnerResult(ok: true, error: nil)))))
    }
    return events
}

private func query(_ nowMs: Double) -> CoreSceneQuery {
    CoreSceneQuery(nowMs: nowMs, reduceMotion: false, viewWidth: 393, viewHeight: 852)
}

@Suite("Live view: the adapter over the core's viewer", .timeLimit(.minutes(1)))
struct LiveViewModelTests {
    @Test("a demo run through the coordinator: one step per command the phone received, captions, gestures and points match")
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
        let all = events.withLock { $0 }
        let counting = try CountingViewer()
        let viewer = LiveViewModel(viewer: counting)
        for e in all { viewer.apply(e) }
        #expect(counting.calls("feed") == all.count, "one FFI feed per update")
        #expect(counting.calls("timeline") <= all.count)
        // The commands the driver actually sent.
        let sent = all.compactMap { e -> RunnerCommand? in
            if case .update(.effect(.act(let command), _)) = e { return command }
            return nil
        }
        #expect(sent.count == 2)
        let t = viewer.timeline
        #expect(t.steps.count == sent.count)
        for (step, command) in zip(t.steps, sent) {
            guard case .tap(let x, let y) = command else {
                Issue.record("demo sent \(command)")
                continue
            }
            #expect(step.gesture == .tap(point: Point(x: x, y: y)))
            #expect(step.state == .done)
            let screen = try #require(t.screen(id: step.screenId))
            #expect(screen.source == .tree)
            #expect(screen.appName == DemoPhone.appName)
            #expect(screen.frameKey == nil, "frames are off by default")
            #expect(viewer.frameKey(for: screen) == nil)
            // Replayed: the cursor ends exactly on the point the phone was sent.
            viewer.replay(.scrubTo(number: step.number), nowMs: 0, reduceMotion: false)
            let scene = try #require(viewer.scene(query(60_000)))
            #expect(scene.stepNumber == step.number)
            #expect(scene.cursor == Point(x: x, y: y))
            #expect(scene.caption == step.caption)
        }
        #expect(t.steps.map(\.caption) == ["Tap \u{201C}Wi-Fi\u{201D}", "Tap \u{201C}Wi-Fi\u{201D}"])
        #expect(t.steps[0].target?.role == "StaticText")
        #expect(t.steps[1].target?.role == "Switch")
        #expect(t.steps[1].target?.frame == Rect(x: 312, y: 166, width: 51, height: 31))
        #expect(t.steps.map(\.number) == [1, 2])
        #expect(t.activity == .finished(outcome: .success))
        #expect(t.activityCaption == "Done")
        #expect(!t.taskId.isEmpty)
        // The wireframe of the first screen: the shown elements, without the app and window.
        let first = try #require(viewer.screen(id: t.steps[0].screenId))
        #expect(first.elements.map(\.role) == ["NavigationBar", "Table", "Cell", "StaticText", "Button"])
        #expect(first.elements.map(\.depth) == [0, 0, 1, 2, 1])
        #expect(coordinator.viewerFrames.count == 0)
        // What the UI test reads: "Step 1 of 2" in replay, the activity once back live.
        let back = try #require(viewer.replay(.scrubTo(number: 1), nowMs: 0, reduceMotion: false))
        #expect(back.positionText == "Step 1 of 2" && !back.following)
        #expect(try #require(viewer.scene(query(0))).accessibilityValue == "Tap \u{201C}Wi-Fi\u{201D}, Step 1 of 2")
        let live = try #require(viewer.replay(.goLive, nowMs: 0, reduceMotion: false))
        #expect(live.following && live.positionText == "Live · 2 steps")
        #expect(try #require(viewer.scene(query(60_000))).accessibilityValue == "Done")
    }

    @Test("every kind of update crosses once: prompts, replies and ignored driver updates included")
    func oneFeedPerUpdate() throws {
        let counting = try CountingViewer()
        let viewer = LiveViewModel(viewer: counting)
        let request = ConfirmRequest(category: .delete, summary: "Delete", app: "com.apple.mobilemail")
        var events = tapRun(3)
        events += [
            .prompt("<<<SCREEN_DATA>>>\nApp: Mail (com.apple.mobilemail)\n<<<END_SCREEN_DATA>>>"),
            .reply("{}"),
            .update(.confirmationPending(request)),
            .update(.questionPending("Which one?")),
            .update(.awaitingUnlock(reason: "locked", waitedMs: 5)),
            .update(.lockDetected(.infer)),
            .update(.unlockWaitEnded),
            .update(.ioFailed(.observe, message: "timeout", willRetry: true)),
            .update(.note("note")),
        ]
        var republished = 0
        for e in events where viewer.apply(e) { republished += 1 }
        #expect(counting.calls("feed") == events.count)
        #expect(counting.calls("timeline") == republished + 1, "fetched once at init and once per change")
        #expect(republished < events.count, "updates that change nothing are not republished")
        #expect(viewer.timeline.steps.count == 3)
        #expect(viewer.timeline.activity == .locked)
        // The mapping is one-to-one.
        #expect(LiveViewModel.update(for: .reply("x")) == .ignored)
        #expect(LiveViewModel.update(for: .update(.note("n"))) == .ignored)
        #expect(LiveViewModel.update(for: .update(.effect(.observe(screenshot: false), number: -1))) == .effect(effect: .observe(screenshot: false), number: 0))
        #expect(LiveViewModel.update(for: .update(.started(TaskSpec(id: "t", text: "x")))) == .started(taskId: "t"))
        // One scene call per drawn frame, and the wireframe is fetched once, then cached.
        let scenesBefore = counting.calls("scene")
        for frame in 0..<10 {
            let scene = try #require(viewer.scene(query(Double(frame) * 16)))
            _ = viewer.screen(id: scene.screenId)
        }
        #expect(counting.calls("scene") == scenesBefore + 10)
        #expect(counting.calls("screen") == 1)
    }

    @Test("1,000 steps on 1,000 screens: the wireframe cache stays bounded and holds only screens the timeline refers to")
    func boundedCache() throws {
        let counting = try CountingViewer()
        let viewer = LiveViewModel(viewer: counting)
        var maxCached = 0
        var drawn = 0
        for e in tapRun(1_000) {
            guard viewer.apply(e) else { continue }
            // Draw like the view: the scene's screen, plus the oldest kept one (a scrub back).
            let t = viewer.timeline
            if let scene = viewer.scene(query(Double(drawn) * 16)) { _ = viewer.screen(id: scene.screenId) }
            _ = viewer.screen(id: t.screens.first?.id)
            drawn += 1
            maxCached = max(maxCached, viewer.cachedScreenCount)
        }
        let t = viewer.timeline
        #expect(t.steps.count == Int(LiveViewModel.maxSteps))
        #expect(t.steps.first?.number == 801 && t.droppedSteps == 800)
        #expect(t.screens.count <= Int(LiveViewModel.maxSteps) + 1)
        #expect(maxCached <= LiveViewModel.maxCachedScreens)
        #expect(maxCached > 0)
        #expect(counting.calls("feed") == 1 + 4 * 1_000)
        // Every cached screen is still in the timeline, and a dropped one is gone.
        let kept = Set(t.screens.map(\.id))
        for id in kept { #expect(viewer.screen(id: id)?.info.id == id) }
        #expect(viewer.cachedScreenCount <= LiveViewModel.maxCachedScreens)
        #expect(viewer.screen(id: 1) == nil, "the first screen was dropped with its steps")
    }

    @Test("FFI errors: nothing crashes, nothing is drawn, and the next update brings the model back")
    func ffiErrors() throws {
        let counting = try CountingViewer()
        let viewer = LiveViewModel(viewer: counting)
        let events = tapRun(2)
        for e in events.prefix(5) { viewer.apply(e) }
        let before = viewer.timeline
        #expect(before.steps.count == 1)

        counting.fail(["feed", "timeline", "scene", "screen", "replay", "replayStatus", "encode"])
        #expect(!viewer.apply(events[5]), "a failed feed reports no change")
        #expect(viewer.timeline == before, "the last good timeline stays")
        #expect(viewer.scene(query(0)) == nil)
        #expect(!viewer.needsFrames(nowMs: 0, reduceMotion: false), "a broken viewer never keeps the display busy")
        #expect(viewer.screen(id: before.steps[0].screenId) == nil)
        #expect(viewer.replay(.stepBack, nowMs: 0, reduceMotion: false) == nil)
        #expect(viewer.replayStatus() == nil)
        #expect(viewer.encode() == nil)

        // The feed works again (the second screen arrives) but fetching the timeline does
        // not: still the old one.
        counting.fail(["timeline"])
        #expect(!viewer.apply(events[6]))
        #expect(viewer.timeline == before)

        // Everything works: the next update, even one that changes nothing, republishes,
        // and what was fed while the fetch failed is there.
        counting.fail([])
        #expect(viewer.apply(.reply("{}")))
        #expect(viewer.timeline.screens.count == before.screens.count + 1)
        #expect(viewer.timeline.steps.count == 1)
        for e in events[7...] { viewer.apply(e) }
        #expect(viewer.timeline.steps.count == 2)
        #expect(viewer.timeline.steps.last?.caption == "Tap \u{201C}Send 1\u{201D}", "the target is on the screen fed during the failure")
        #expect(viewer.scene(query(0)) != nil)
        #expect(viewer.encode() != nil)
        #expect(viewer.replay(.stepBack, nowMs: 0, reduceMotion: false)?.positionText == "Step 1 of 2")
    }

    @Test("without a viewer (the core refused to make one) the model is empty and every call is safe")
    func noViewer() {
        let viewer = LiveViewModel(viewer: nil)
        for e in tapRun(2) { #expect(!viewer.apply(e)) }
        #expect(viewer.timeline == .empty)
        #expect(viewer.scene(query(0)) == nil)
        #expect(viewer.screen(id: 1) == nil)
        #expect(viewer.replay(.reset, nowMs: 0, reduceMotion: false) == nil)
        #expect(viewer.encode() == nil)
        #expect(!viewer.needsFrames(nowMs: 0, reduceMotion: true))
        #expect(LiveViewModel().timeline == .empty, "a fresh viewer's timeline is the empty one")
    }

    @Test("frame keys convert losslessly from the core's; frames on gives each unlocked screen one")
    func frameKeys() throws {
        let key = try #require(ViewerFrameKey(CoreViewerFrameKey(taskId: "t", observation: 42)))
        #expect(key == ViewerFrameKey(taskId: "t", observation: 42))
        #expect(ViewerFrameKey(CoreViewerFrameKey(taskId: "t", observation: UInt64.max)) == nil)
        let viewer = LiveViewModel(framesEnabled: true)
        for e in tapRun(2, taskId: "frames") { viewer.apply(e) }
        let t = viewer.timeline
        let keys = t.steps.compactMap { viewer.frameKey(for: t.screen(id: $0.screenId)) }
        #expect(keys == [ViewerFrameKey(taskId: "frames", observation: 1), ViewerFrameKey(taskId: "frames", observation: 3)])
    }

    @Test("wireframe rectangles map into the view with the scene's mapping, clipped to the screen")
    func viewRects() throws {
        let viewer = LiveViewModel()
        for e in tapRun(1) { viewer.apply(e) }
        // A view twice the screen's size: scale 2, no letterbox.
        let scene = try #require(viewer.scene(CoreSceneQuery(nowMs: 0, reduceMotion: false, viewWidth: 786, viewHeight: 1704)))
        #expect(scene.scale == 2 && scene.origin == Point(x: 0, y: 0))
        #expect(scene.viewRect(Rect(x: 16, y: 400, width: 361, height: 44)) == Rect(x: 32, y: 800, width: 722, height: 88))
        #expect(scene.viewRect(Rect(x: -10, y: -10, width: 20, height: 20)) == Rect(x: 0, y: 0, width: 20, height: 20))
        #expect(scene.viewRect(Rect(x: 500, y: 0, width: 10, height: 10)) == nil, "off the screen")
        #expect(scene.viewRect(Rect(x: 0, y: 0, width: 0, height: 10)) == nil, "empty")
        #expect(scene.viewRect(Rect(x: .nan, y: 0, width: 10, height: 10)) == nil)
        // Letterboxed: a wide view centres the screen.
        let wide = try #require(viewer.scene(CoreSceneQuery(nowMs: 0, reduceMotion: false, viewWidth: 1000, viewHeight: 852)))
        #expect(wide.origin.x > 0 && wide.viewRect(Rect(x: 0, y: 0, width: 393, height: 852)) == wide.content)
    }
}
