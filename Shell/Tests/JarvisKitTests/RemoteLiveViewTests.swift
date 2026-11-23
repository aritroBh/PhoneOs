import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The Live view over remote control through the real hub, client and viewers: the phone's
// `RemoteControlService.feedLiveView` (one FFI call per driver update) and a paired
// controller's `RemoteLiveView` over `InMemoryRemoteTransport`.

/// A hub clock that moves 100 ms per reading, so every screen clears the hub's one screen
/// per 250 ms (a real clock in a fast test would let screens replace each other, which the
/// stream allows and documents) while a whole run stays far inside the idle timeout.
private final class SteppingMs: Sendable {
    private let now = Mutex<UInt64>(1_000_000)
    func next() -> UInt64 { now.withLock { $0 += 100; return $0 } }
}

/// The phone's Live view as the app keeps it: a fresh `LiveViewModel` per session, every
/// run event fed through the service (as `AppModel.feedViewer` does).
@MainActor
private final class PhoneLiveView {
    var viewer = LiveViewModel()
    private var observer: UUID?

    init(_ r: RemoteHarness) {
        let service = r.service
        observer = r.base.center.observe { [weak self] event in
            guard let self else { return }
            switch event {
            case .sessionStarted: self.viewer = LiveViewModel()
            case .session(_, let runEvent): service.feedLiveView(self.viewer, runEvent)
            default: break
            }
        }
    }
}

@MainActor
private func harness(_ name: String) -> RemoteHarness {
    let clock = SteppingMs()
    return RemoteHarness(name, clock: { clock.next() })
}

/// The timeline as a controller can know it (no generation, no frame keys).
private func comparable(_ t: CoreViewerTimeline, screens: Bool = true) -> CoreViewerTimeline {
    var t = t
    t.generation = 0
    t.screens = screens ? t.screens.map { var s = $0; s.frameKey = nil; return s } : []
    return t
}

/// The scene at a replay position, without the per-viewer generation.
private func scene(_ model: LiveViewModel, step: UInt32, nowMs: Double, width: Double) -> CoreLiveScene? {
    model.replay(.scrubTo(number: step), nowMs: nowMs, reduceMotion: true)
    guard var s = model.scene(CoreSceneQuery(nowMs: nowMs, reduceMotion: true, viewWidth: width, viewHeight: width * 2)) else { return nil }
    s.generation = 0
    return s
}

/// The controller draws what the phone draws: the timeline, the wireframes it holds, and
/// the scene for random replay queries (seeded). With `everyScreen` false (after a
/// snapshot) only the screens the controller holds are compared: a snapshot carries the
/// newest steps without their older screens (documented in the core).
@MainActor
private func expectSame(_ phone: LiveViewModel, _ controller: LiveViewModel, everyScreen: Bool = true) throws {
    let p = phone.timeline
    let c = controller.timeline
    #expect(comparable(p, screens: everyScreen) == comparable(c, screens: everyScreen))
    for info in c.screens {
        #expect(p.screens.contains { $0.id == info.id })
        #expect(phone.screen(id: info.id)?.elements == controller.screen(id: info.id)?.elements)
    }
    let held = Set(c.screens.map(\.id))
    let numbers = p.steps.filter { $0.screenId.map(held.contains) ?? true }.map(\.number)
    // After a snapshot the older steps' screens are not held; only the steps drawn on what
    // the controller holds can be compared scene by scene.
    if everyScreen { try #require(!numbers.isEmpty) }
    guard !numbers.isEmpty else { return }
    var generator = SplitMix(seed: 7)
    for _ in 0..<40 {
        let step = numbers[Int(generator.next() % UInt64(numbers.count))]
        let now = Double(generator.next() % 20_000)
        let width = Double(200 + generator.next() % 300)
        #expect(scene(phone, step: step, nowMs: now, width: width) == scene(controller, step: step, nowMs: now, width: width),
                "step \(step) at \(now)")
    }
    phone.replay(.goLive, nowMs: 30_000, reduceMotion: true)
    controller.replay(.goLive, nowMs: 30_000, reduceMotion: true)
}

private struct SplitMix {
    var state: UInt64
    init(seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}

@MainActor
private func runWifiOff(_ r: RemoteHarness) async throws {
    guard case .success(let task) = r.base.center.start("Turn off Wi-Fi") else {
        Issue.record("the task did not start")
        return
    }
    let outcome = await task.value
    #expect(outcome.outcome.status == .success)
}

// MARK: - Synthetic runs (exact screens: secrets, a locked phone, large wireframes)

private let fullScreen = Rect(x: 0, y: 0, width: 393, height: 852)

private func node(_ role: Role, _ label: String?, _ frame: Rect, secure: Bool = false, _ children: [UiNode] = []) -> UiNode {
    UiNode(role: role, label: label, value: nil, identifier: nil, placeholder: nil, frame: frame, enabled: true, visible: true,
           hittable: role == .button || role == .textField || role == .secureTextField, focused: false, selected: false,
           secure: secure, children: children)
}

/// A driver's updates for a made-up run, in the driver's order.
private struct SyntheticRun {
    var events: [RunEvent] = []
    private var number = 0

    init(task: String) { events.append(.update(.started(TaskSpec(id: task, text: task)))) }

    mutating func observe(_ root: UiNode, locked: Bool = false, app: String = "Example") {
        number += 1
        let screen = Screen(appBundleId: "com.example.app", appName: app, size: Size(width: 393, height: 852), root: root,
                            keyboardVisible: false, orientation: .portrait)
        events.append(.update(.effect(.observe(screenshot: false), number: number)))
        events.append(.update(.effectCompleted(.observe(screenshot: false),
                                               event: .observed(observation: ScreenObservation(screen: screen, screenshot: nil,
                                                                                              deviceLocked: locked)))))
    }

    mutating func act(_ command: RunnerCommand) {
        number += 1
        events.append(.update(.effect(.act(command: command), number: number)))
        events.append(.update(.effectCompleted(.act(command: command), event: .actResult(result: RunnerResult(ok: true, error: nil)))))
    }
}

/// A flat screen of `count` buttons (depth 2, far inside `UiTreeBuilder.maxDepth`).
private func buttons(_ count: Int, salt: String) -> UiNode {
    let children = (0..<count).map { i in
        node(.button, "\(salt) control \(i)", Rect(x: Double(i % 10) * 39, y: 60 + Double(i / 10) * 19, width: 38, height: 18))
    }
    return node(.application, "App", fullScreen, children)
}

@Suite("Live view over remote control: phone service and controller view", .timeLimit(.minutes(2)))
@MainActor
struct RemoteLiveViewTests {
    @Test("without \u{201C}Watch the screen layout\u{201D} a device is refused and gets no byte of the run")
    func deniedSendsNothing() async throws {
        let r = harness("live-denied")
        defer { r.cleanUp() }
        let phone = PhoneLiveView(r)
        let remote = try await r.pairDevice()
        let deviceId = try #require(remote.deviceId)
        #expect(!r.service.watchScreen(deviceId), "off by default")
        let view = RemoteLiveView()
        try remote.startWatching(view)
        try await eventually("the refusal") { view.state == .denied }
        #expect(r.service.audit.entries.contains { $0.text == "Refused to show the screen layout: not allowed for this device" && $0.isWarning })
        let before = r.transport.sentBytes(on: remote.endpoint.id)
        try await runWifiOff(r)
        #expect(r.transport.sentBytes(on: remote.endpoint.id) == before, "bytes went to a device without the permission")
        #expect(view.model.timeline.steps.isEmpty)
        #expect(!phone.viewer.timeline.steps.isEmpty, "the phone's own view was fed")
        #expect(!r.service.isWatching(deviceId))
        remote.close()
    }

    @Test("with the permission the controller's scene equals the phone's for a demo run; off ends it at once, audited")
    func watchThenRevoke() async throws {
        let r = harness("live-watch")
        defer { r.cleanUp() }
        let phone = PhoneLiveView(r)
        let remote = try await r.pairDevice()
        let deviceId = try #require(remote.deviceId)
        try r.service.setWatchScreen(deviceId: deviceId, true)
        #expect(r.service.watchScreen(deviceId))
        #expect(r.service.devices.first?.watchScreen == true)
        #expect(r.service.devices.first?.permissions == (try newDeviceRemotePermissions()), "nothing else changed")
        #expect(r.service.audit.entries.contains { $0.text == "Watch the screen layout: on" })
        #expect(try r.secrets.read(RemoteControlService.secretKey)?.contains("\"watch_screen\":true") == true,
                "the permission is stored with the device")
        let view = RemoteLiveView()
        try remote.startWatching(view)
        try await eventually("the snapshot") { view.state == .watching }
        try await eventually("watching on the phone") { r.service.isWatching(deviceId) }
        #expect(r.service.audit.entries.contains { $0.text == "Started watching the screen layout" })
        let start = r.transport.sentBytes(on: remote.endpoint.id)

        try await runWifiOff(r)
        r.service.tick()
        try await eventually("the controller to catch up") {
            comparable(view.model.timeline) == comparable(phone.viewer.timeline)
        }
        try expectSame(phone.viewer, view.model)
        #expect(view.counters.resyncs == 0)
        let streamed = r.transport.sentBytes(on: remote.endpoint.id) - start
        print("RemoteLiveViewTests: demo run (Turn off Wi-Fi) streamed \(streamed) bytes in \(view.counters.deltas) deltas")
        // Nothing that is not the timeline: no frame keys, no approval picture.
        let wire = String(decoding: try #require(view.model.encode()), as: UTF8.self)
        #expect(!wire.contains("frame_key"))
        #expect(view.model.approvalTarget == nil)

        try r.service.setWatchScreen(deviceId: deviceId, false)
        try await eventually("the stream to end") { view.state == .ended(.permissionRevoked) }
        #expect(!r.service.isWatching(deviceId))
        #expect(r.service.audit.entries.contains { $0.text == "Watch the screen layout: off" })
        #expect(r.service.audit.entries.contains { $0.text == "Stopped watching the screen layout: the permission was turned off" })
        let afterOff = r.transport.sentBytes(on: remote.endpoint.id)
        try await runWifiOff(r)
        #expect(r.transport.sentBytes(on: remote.endpoint.id) == afterOff, "the stream went on after the permission was turned off")
        remote.close()
    }

    @Test("dropped deltas cost one resync and the controller converges")
    func droppedFramesResync() async throws {
        let r = harness("live-resync")
        defer { r.cleanUp() }
        let phone = PhoneLiveView(r)
        let remote = try await r.pairDevice()
        try r.service.setWatchScreen(deviceId: try #require(remote.deviceId), true)
        let view = RemoteLiveView()
        try remote.startWatching(view)
        try await eventually("the snapshot") { view.state == .watching }
        view.dropNextDeltas(2)
        try await runWifiOff(r)
        r.service.tick()
        try await eventually("the controller to converge") {
            comparable(view.model.timeline, screens: false) == comparable(phone.viewer.timeline, screens: false)
                && !view.model.needsResync
        }
        try expectSame(phone.viewer, view.model, everyScreen: false)
        #expect(view.counters.resyncs == 1, "one snapshot request per gap")
        remote.close()
    }

    @Test("a reconnect starts from a snapshot")
    func reconnectSnapshot() async throws {
        let r = harness("live-reconnect")
        defer { r.cleanUp() }
        let phone = PhoneLiveView(r)
        let first = try await r.pairDevice()
        let deviceId = try #require(first.deviceId)
        let credentials = try #require(first.credentials)
        try r.service.setWatchScreen(deviceId: deviceId, true)
        try await runWifiOff(r)
        first.close()
        try await eventually("the closed connection") { !r.service.isConnected(deviceId) }
        let remote = try await InProcessRemote.connect(credentials: credentials, over: r.transport)
        let view = RemoteLiveView()
        try remote.startWatching(view)
        try await eventually("the snapshot") {
            view.state == .watching
                && comparable(view.model.timeline, screens: false) == comparable(phone.viewer.timeline, screens: false)
        }
        #expect(view.counters.snapshotParts >= 2, "a reset and the steps")
        #expect(view.counters.deltas == 0, "nothing but the snapshot")
        try expectSame(phone.viewer, view.model, everyScreen: false)
        remote.close()
    }

    @Test("a device that never reads never blocks or slows the run, and the phone keeps a bounded amount for it")
    func stalledConnectionBounded() async throws {
        let r = harness("live-stalled")
        defer { r.cleanUp() }
        let remote = try await r.pairDevice()
        try r.service.setWatchScreen(deviceId: try #require(remote.deviceId), true)
        let view = RemoteLiveView()
        try remote.startWatching(view)
        try await eventually("the snapshot") { view.state == .watching }
        r.transport.holdWrites(true, on: remote.endpoint.id)
        let start = r.transport.sentBytes(on: remote.endpoint.id)
        // 200 steps, each on a new 300-element screen: megabytes of timeline.
        var run = SyntheticRun(task: "big")
        for i in 0..<200 {
            run.observe(buttons(300, salt: "s\(i)"))
            run.act(.tap(x: 10, y: 70))
        }
        let viewer = LiveViewModel()
        let began = ContinuousClock.now
        for event in run.events { r.service.feedLiveView(viewer, event) }
        let elapsed = ContinuousClock.now - began
        let held = r.transport.sentBytes(on: remote.endpoint.id) - start
        print("RemoteLiveViewTests: stalled device got \(held) bytes of a \(run.events.count)-update run in \(elapsed)")
        // The hub's window (128 KiB) plus what crossed it (one frame or one snapshot).
        #expect(held < 128 * 1_024 + 400 * 1_024, "\(held) bytes handed to a connection that confirms nothing")
        #expect(elapsed < .seconds(20))
        #expect(viewer.timeline.steps.count == 200)
        // And a real run is not slowed by it.
        try await runWifiOff(r)
        #expect(r.transport.sentBytes(on: remote.endpoint.id) - start == held, "more was handed out without confirmation")
        remote.close()
    }

    @Test("the stream carries no secret, no picture and nothing of a locked screen")
    func privacy() async throws {
        let r = harness("live-privacy")
        defer { r.cleanUp() }
        let remote = try await r.pairDevice()
        try r.service.setWatchScreen(deviceId: try #require(remote.deviceId), true)
        let view = RemoteLiveView()
        try remote.startWatching(view)
        try await eventually("the snapshot") { view.state == .watching }
        var run = SyntheticRun(task: "private")
        let shown = SecretCorpus.all.enumerated().map { i, sample in
            node(.button, sample.shown, Rect(x: 16, y: Double(100 + 60 * i), width: 361, height: 44))
        }
        run.observe(node(.application, "Shop", fullScreen, [node(.window, nil, fullScreen, shown)]))
        run.act(.tap(x: 100, y: 120))
        let field = node(.secureTextField, "Password", Rect(x: 16, y: 200, width: 361, height: 44), secure: true)
        run.observe(node(.application, "Shop", fullScreen, [node(.window, nil, fullScreen, [field])]))
        for sample in SecretCorpus.all { run.act(.typeText(text: sample.secret)) }
        // The phone locks: a lock screen full of private things.
        run.observe(node(.application, "Lock", fullScreen, [node(.staticText, "Balance 12,345.67", Rect(x: 16, y: 300, width: 300, height: 30)),
                                                            node(.staticText, "Unlock code 4242", Rect(x: 16, y: 340, width: 300, height: 30))]),
                    locked: true)
        let phone = LiveViewModel()
        for event in run.events { r.service.feedLiveView(phone, event) }
        r.service.tick()
        try await eventually("the controller to catch up") {
            view.model.timeline.steps.count == phone.timeline.steps.count && view.model.timeline.currentScreenId == phone.timeline.currentScreenId
                && view.model.screen(id: phone.timeline.currentScreenId) != nil
        }
        let wire = String(decoding: try #require(view.model.encode()), as: UTF8.self)
        for sample in SecretCorpus.all {
            #expect(!wire.contains(sample.secret), "\(sample.secret) reached the controller")
        }
        #expect(!wire.contains("12,345") && !wire.contains("4242") && !wire.contains("Balance"))
        let current = try #require(view.model.screen(id: view.model.timeline.currentScreenId))
        #expect(current.info.source == .locked && current.elements.isEmpty, "the locked screen is an empty outline")
        #expect(view.model.timeline.screens.allSatisfy { $0.frameKey == nil }, "no picture is named")
        #expect(view.model.approvalTarget == nil)
        remote.close()
    }

    @Test("the network transport's unsent limit leaves room for the hub's window plus one snapshot")
    func outboundRoom() {
        let window = 128 * 1_024
        let snapshot = (256 * 1_024 + 2) / 3 * 4 + 16 * 128
        let frame = RemoteTransportLimits.coreMaxBufferedBytes
        #expect(RemoteTransportLimits.maxUnsentOutboundBytes >= window + snapshot + 2 * frame)
    }

    @Test("audit words for the Live view stream")
    func auditWords() {
        func text(_ e: AuditEvent) -> String { RemoteAuditText.describe(e).text }
        func warning(_ e: AuditEvent) -> Bool { RemoteAuditText.describe(e).warning }
        #expect(text(.accepted(request: "start_watching", detail: nil)) == "Started watching the screen layout")
        #expect(text(.accepted(request: "stop_watching", detail: nil)) == "Stopped watching the screen layout")
        #expect(text(.accepted(request: "stop_watching", detail: "permission revoked"))
                    == "Stopped watching the screen layout: the permission was turned off")
        #expect(text(.accepted(request: "watch_screen_permission", detail: "on")) == "Watch the screen layout: on")
        #expect(text(.accepted(request: "watch_screen_permission", detail: "off")) == "Watch the screen layout: off")
        #expect(!warning(.accepted(request: "watch_screen_permission", detail: "off")))
        #expect(text(.denied(request: "start_watching", reason: .notPermitted))
                    == "Refused to show the screen layout: not allowed for this device")
        #expect(warning(.denied(request: "start_watching", reason: .notPermitted)))
        #expect(warning(.denied(request: "viewer_resync", reason: .notPermitted)))
        // Everything else reads as before.
        #expect(text(.accepted(request: "start_task", detail: "x")) == "Accepted start_task: x")
        #expect(text(.denied(request: "decision", reason: .busy)) == "Refused decision: Jarvis is busy")
    }
}
