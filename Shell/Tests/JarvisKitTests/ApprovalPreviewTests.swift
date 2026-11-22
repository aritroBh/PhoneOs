import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The approval sheet's real screen ("Show the real screen when Jarvis asks"): the driver's
// best-effort capture of the screen a confirmation was computed from, bound to that
// observation, held only while the question is open, and never able to change the run;
// the preview model built from the core's picture (`JarvisViewer.approvalTarget()`); and
// nothing of it reaching the encoded timeline, the history or a paired device.

private let approvalPixels = CapturedScreenshot(data: Data(repeating: 0xA7, count: 48), mimeType: "image/png",
                                                pixelWidth: 393, pixelHeight: 852)
private let corePixels = CapturedScreenshot(data: Data(repeating: 0xC3, count: 40), mimeType: "image/jpeg",
                                            pixelWidth: 393, pixelHeight: 852)

/// The demo phone with a real-screen capture; counts every runner call and can make the
/// screen move on, or the phone lock, right after a capture.
private final class ApprovalPhone: PhoneRunner, Sendable {
    enum Capture: Sendable { case pixels, hangs, none }

    private struct State {
        var snapshots = 0
        var captures = 0
        var capturesWhileLocked = 0
        var lockedChecks = 0
        var markNext = false
        var lockNext = false
    }

    let inner: DemoPhone
    let capture: Capture
    /// Replaces the lock report of unlocked observations when set.
    let reportedLock: DeviceLockState?
    /// After a capture, the next tree read shows a changed screen (the core's own
    /// observations are never changed).
    let moveOnAfterCapture: Bool
    /// After a capture, the next tree read reports the phone locked.
    let lockAfterCapture: Bool
    private let state = Mutex(State())

    init(_ capture: Capture = .pixels, reportedLock: DeviceLockState? = nil, moveOnAfterCapture: Bool = false,
         lockAfterCapture: Bool = false) {
        self.inner = DemoPhone()
        self.capture = capture
        self.reportedLock = reportedLock
        self.moveOnAfterCapture = moveOnAfterCapture
        self.lockAfterCapture = lockAfterCapture
    }

    var snapshots: Int { state.withLock { $0.snapshots } }
    var captures: Int { state.withLock { $0.captures } }
    var capturesWhileLocked: Int { state.withLock { $0.capturesWhileLocked } }
    var commands: [RunnerCommand] { inner.commands }

    /// The phone reports itself locked for the next `checks` lock-state reads (and its
    /// snapshots show the lock screen meanwhile).
    func lock(forChecks checks: Int) { state.withLock { $0.lockedChecks = checks } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let (locked, mark, lockNow) = state.withLock { s -> (Bool, Bool, Bool) in
            s.snapshots += 1
            defer { s.markNext = false; s.lockNext = false }
            return (s.lockedChecks > 0, s.markNext, s.lockNext)
        }
        if locked {
            return RunnerObservation(root: DemoPhone.tree(for: .home), appBundleId: "com.apple.springboard", appName: nil,
                                     screenSize: DemoPhone.screenSize, deviceLocked: true)
        }
        var o = try await inner.snapshot(includeScreenshot: includeScreenshot)
        if let reportedLock { o.lockState = reportedLock }
        if includeScreenshot { o.screenshot = corePixels }
        if lockNow { o.lockState = .locked; o.deviceLocked = true }
        if mark, var root = o.root {
            root.children.append(UiNode(role: .staticText, label: "Updated just now", value: nil, identifier: nil,
                                        placeholder: nil, frame: Rect(x: 16, y: 800, width: 200, height: 20), enabled: true,
                                        visible: true, hittable: false, focused: false, selected: false, secure: false,
                                        children: []))
            o.ui = .tree(root)
        }
        return o
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }

    func lockState() async throws -> DeviceLockState {
        state.withLock { s in
            guard s.lockedChecks > 0 else { return .unlocked }
            s.lockedChecks -= 1
            return .locked
        }
    }

    func viewerScreenshot() async throws -> CapturedScreenshot? {
        state.withLock { s in
            s.captures += 1
            if s.lockedChecks > 0 { s.capturesWhileLocked += 1 }
            s.markNext = moveOnAfterCapture
            s.lockNext = lockAfterCapture
        }
        switch capture {
        case .pixels: return approvalPixels
        case .hangs:
            try await Task.sleep(for: .seconds(30))
            return approvalPixels
        case .none: return nil
        }
    }
}

/// An owner who answers each confirmation as scripted (the last reply repeats) and records
/// what the approval store held while each question was open.
private final class ApprovalGate: HumanGate {
    enum Reply: Sendable { case approve, decline, fail, hang }

    private let replies: [Reply]
    private let store: ApprovalFrameStore?
    private let onAsk: (@Sendable (Int) -> Void)?
    private let log = Mutex<(asked: [(key: ViewerFrameKey?, frame: CapturedScreenshot?)], at: [ContinuousClock.Instant])>(([], []))

    init(_ replies: [Reply] = [.approve], store: ApprovalFrameStore?, onAsk: (@Sendable (Int) -> Void)? = nil) {
        self.replies = replies
        self.store = store
        self.onAsk = onAsk
    }

    var asked: [(key: ViewerFrameKey?, frame: CapturedScreenshot?)] { log.withLock { $0.asked } }
    var askedAt: [ContinuousClock.Instant] { log.withLock { $0.at } }

    func confirm(_ request: ConfirmRequest) async throws -> Bool {
        let n = log.withLock { l -> Int in
            let key = store?.key
            l.asked.append((key, key.flatMap { store?.frame(for: $0) }))
            l.at.append(ContinuousClock.now)
            return l.asked.count
        }
        onAsk?(n)
        switch replies[min(n - 1, replies.count - 1)] {
        case .approve: return true
        case .decline: return false
        case .fail: throw InjectedError(message: "no confirmation UI")
        case .hang:
            try await Task.sleep(for: .seconds(60))
            return true
        }
    }

    func answer(_ question: String) async throws -> String { "" }
}

private struct ApprovalRun {
    var result: RunResult
    var updates: [DriverUpdate]

    /// The number of the observation each confirmation was computed from (the last
    /// observation the core was given before it).
    var evaluated: [Int] {
        var out: [Int] = []
        var number = 0
        var lastObserved = 0
        for u in updates {
            if case .effect(let effect, let n) = u {
                number = n
                if case .confirm = effect { out.append(lastObserved) }
            }
            if case .effectCompleted(_, let event) = u {
                switch event {
                case .observed, .observedRunnerText: lastObserved = number
                default: break
                }
            }
        }
        return out
    }

    var observationCount: Int {
        updates.filter {
            guard case .effectCompleted(_, let event) = $0 else { return false }
            switch event {
            case .observed, .observedRunnerText: return true
            default: return false
            }
        }.count
    }

    /// The run's events as the app forwards them (to the Live view, the coordinator).
    var events: [RunEvent] { updates.map { .update($0) } }
}

private func driveApproval(
    _ phone: any PhoneRunner, gate: ApprovalGate, store: ApprovalFrameStore?, option: Bool = true,
    timeout: Duration = .seconds(1), humanTimeout: Duration? = .seconds(600), lockWatch: Duration? = nil,
    viewerFrames: ViewerFrameStore? = nil, vision: Bool = false, cancelAfter: Duration? = nil
) async throws -> ApprovalRun {
    let model = ScriptedModel(steps: DemoPhone.deleteAccountScript,
                              capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: vision))
    let config = try SessionConfigBuilder.make(capabilities: model.capabilities, knownApps: DemoPhone.knownApps,
                                               adjustPolicy: { $0.firstUseApproval = false; $0.maxSteps = 40 })
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: gate, memory: RecordingMemory(),
                                  screenshots: ScreenshotStore(), clock: SteppingClock(), viewerFrames: viewerFrames,
                                  approvalFrames: store)
    let driver = try SessionDriver(
        config: config, task: TaskSpec(id: "ask", text: "Delete my account"), memoryJSON: "", dependencies: deps,
        configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10), humanTimeout: humanTimeout,
                                           unlockPollInterval: .milliseconds(5), lockWatchInterval: lockWatch,
                                           viewerScreenshots: viewerFrames != nil, viewerScreenshotTimeout: timeout,
                                           approvalScreenshots: option)
    )
    let collector = Task { () -> [DriverUpdate] in
        var all: [DriverUpdate] = []
        for await u in driver.updates { all.append(u) }
        return all
    }
    if let cancelAfter {
        Task {
            try? await Task.sleep(for: cancelAfter)
            await driver.cancel()
        }
    }
    let result = await driver.run()
    return ApprovalRun(result: result, updates: await collector.value)
}

@Suite("Approval preview: the driver's capture", .timeLimit(.minutes(2)), .serialized)
struct ApprovalCaptureTests {
    @Test("the frame is of the observation the confirmation was computed from, held while asked, gone after")
    func boundToTheEvaluatedObservation() async throws {
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store)
        let evaluated = try #require(run.evaluated.first)
        #expect(gate.asked.count == 1)
        #expect(gate.asked.first?.key == ViewerFrameKey(taskId: "ask", observation: evaluated))
        #expect(gate.asked.first?.frame == approvalPixels)
        #expect(phone.captures == 1, "one capture, only for the question")
        #expect(store.count == 0, "dropped once answered")

        // The core's picture names the same observation: the preview shows the real screen
        // while the frame is held under that key, and the layout otherwise.
        let viewer = LiveViewModel()
        for e in run.events {
            viewer.apply(e)
            if case .update(.confirmationPending) = e { break }
        }
        let target = try #require(viewer.approvalTarget)
        #expect(ViewerFrameKey(target.frameKey) == ViewerFrameKey(taskId: "ask", observation: evaluated))
        let prompt = HumanPrompt.confirm(id: 7, request: target.request)
        store.put(approvalPixels, for: ViewerFrameKey(taskId: "ask", observation: evaluated))
        let real = try #require(ApprovalPreview.make(prompt: prompt, target: target, frames: store, appActive: true))
        #expect(real.backdrop == .realScreen && real.badge == "What Jarvis sees now")
        #expect(real.frame(in: store) == approvalPixels)
        #expect(ApprovalPreview.make(prompt: prompt, target: target, frames: store, appActive: false)?.backdrop == .layout,
                "never the real screen while the app is not in front")
        store.put(approvalPixels, for: ViewerFrameKey(taskId: "ask", observation: evaluated - 1))
        let other = try #require(ApprovalPreview.make(prompt: prompt, target: target, frames: store, appActive: true))
        #expect(other.backdrop == .layout && other.badge == "Layout only", "a frame of another observation is never shown")
        #expect(other.frame(in: store) == nil)
        // Only for the confirmation the picture belongs to.
        let wrong = ConfirmRequest(category: target.request.category, summary: "Something else", app: target.request.app)
        #expect(ApprovalPreview.make(prompt: .confirm(id: 7, request: wrong), target: target, frames: store, appActive: true) == nil)
        #expect(ApprovalPreview.make(prompt: .question(id: 7, text: "?"), target: target, frames: store, appActive: true) == nil)
        #expect(real.isLive(pending: prompt) && !real.isLive(pending: .confirm(id: 8, request: target.request)))
        #expect(!real.isLive(pending: nil))
    }

    @Test("the screen moved on after it was evaluated: no frame, the layout is shown")
    func movedOn() async throws {
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone(moveOnAfterCapture: true)
        let gate = ApprovalGate([.decline], store: store)
        _ = try await driveApproval(phone, gate: gate, store: store)
        #expect(phone.captures == 1)
        #expect(gate.asked.count == 1 && gate.asked.first?.key == nil && gate.asked.first?.frame == nil)
        #expect(store.count == 0)
    }

    @Test("pixels of that very observation are reused: the core's, or the Live view's frame of it")
    func reused() async throws {
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store, vision: true)
        let evaluated = try #require(run.evaluated.first)
        // Whether the core asked pixels of the evaluated observation.
        var number = 0
        var asked = false
        for u in run.updates {
            if case .effect(let effect, let n) = u {
                number = n
                if n == evaluated, case .observe(let s) = effect { asked = s }
            }
        }
        _ = number
        if asked {
            #expect(gate.asked.first?.frame == corePixels, "the core's pixels of the evaluated screen")
            #expect(phone.captures == 0, "no second capture")
        } else {
            #expect(gate.asked.first?.frame == approvalPixels)
            #expect(phone.captures == 1)
        }

        let frames = ViewerFrameStore()
        let livePhone = ApprovalPhone()
        let liveGate = ApprovalGate([.decline], store: store)
        let liveRun = try await driveApproval(livePhone, gate: liveGate, store: store, viewerFrames: frames)
        let liveEvaluated = try #require(liveRun.evaluated.first)
        #expect(liveGate.asked.first?.key == ViewerFrameKey(taskId: "ask", observation: liveEvaluated))
        #expect(liveGate.asked.first?.frame == approvalPixels)
        #expect(livePhone.captures == liveRun.observationCount, "the Live view's captures only, none for the question")
        #expect(store.count == 0)
    }

    @Test("dropped on every resolution: approved, declined, failed, timed out, cancelled, a lock")
    func droppedOnEveryResolution() async throws {
        for reply: ApprovalGate.Reply in [.approve, .decline, .fail] {
            let store = ApprovalFrameStore()
            let gate = ApprovalGate([reply], store: store)
            _ = try await driveApproval(ApprovalPhone(), gate: gate, store: store)
            #expect(gate.asked.first?.frame == approvalPixels, "\(reply)")
            #expect(store.count == 0, "\(reply)")
        }
        // Timed out.
        let timedStore = ApprovalFrameStore()
        let timedGate = ApprovalGate([.hang], store: timedStore)
        _ = try await driveApproval(ApprovalPhone(), gate: timedGate, store: timedStore, humanTimeout: .milliseconds(80))
        #expect(timedGate.asked.first?.frame == approvalPixels)
        #expect(timedStore.count == 0)
        // The run cancelled while the owner is asked.
        let cancelStore = ApprovalFrameStore()
        let cancelGate = ApprovalGate([.hang], store: cancelStore)
        let cancelled = try await driveApproval(ApprovalPhone(), gate: cancelGate, store: cancelStore, cancelAfter: .milliseconds(300))
        #expect(cancelled.result.endReason == .cancelled)
        #expect(cancelGate.asked.count == 1 && cancelGate.asked.first?.frame == approvalPixels)
        #expect(cancelStore.count == 0)
        // The phone locks while the owner is asked: the question is abandoned, then asked
        // again after the unlock with a fresh frame, and that one is dropped too.
        let lockStore = ApprovalFrameStore()
        let lockPhone = ApprovalPhone()
        let lockGate = ApprovalGate([.hang, .decline], store: lockStore, onAsk: { n in if n == 1 { lockPhone.lock(forChecks: 2) } })
        let locked = try await driveApproval(lockPhone, gate: lockGate, store: lockStore, lockWatch: .milliseconds(20))
        #expect(locked.updates.contains { if case .lockDetected(.confirm) = $0 { true } else { false } })
        #expect(lockGate.asked.count == 2)
        #expect(lockGate.asked.allSatisfy { $0.frame == approvalPixels })
        #expect(lockPhone.capturesWhileLocked == 0, "never captured while the phone is locked")
        #expect(lockStore.count == 0)
        // The app left the foreground (or the phone locked) while the sheet was up.
        let appStore = ApprovalFrameStore()
        appStore.put(approvalPixels, for: ViewerFrameKey(taskId: "t", observation: 1))
        appStore.isAppActive = false
        #expect(appStore.count == 0 && !appStore.accepts)
        #expect(!appStore.put(approvalPixels, for: ViewerFrameKey(taskId: "t", observation: 1)))
        appStore.isAppActive = true
        #expect(appStore.put(approvalPixels, for: ViewerFrameKey(taskId: "t", observation: 1)))
        appStore.remove(ViewerFrameKey(taskId: "t", observation: 2))
        #expect(appStore.count == 1, "removing another observation's key leaves the frame")
        appStore.isEnabled = false
        #expect(appStore.count == 0)
    }

    @Test("never captured unless the phone is known unlocked; a lock seen after the capture keeps nothing")
    func lockRules() async throws {
        let unknownStore = ApprovalFrameStore()
        let unknownPhone = ApprovalPhone(reportedLock: .unknown)
        let unknownGate = ApprovalGate([.decline], store: unknownStore)
        let unknown = try await driveApproval(unknownPhone, gate: unknownGate, store: unknownStore)
        #expect(unknownPhone.captures == 0, "a runner that cannot tell its lock state gets no capture")
        #expect(unknownPhone.snapshots == unknown.observationCount, "and no extra tree read")
        #expect(unknownGate.asked.first?.frame == nil)

        let lockingStore = ApprovalFrameStore()
        let lockingPhone = ApprovalPhone(lockAfterCapture: true)
        let lockingGate = ApprovalGate([.decline], store: lockingStore)
        _ = try await driveApproval(lockingPhone, gate: lockingGate, store: lockingStore)
        #expect(lockingPhone.captures == 1)
        #expect(lockingGate.asked.first?.frame == nil)
        #expect(lockingStore.count == 0)
    }

    @Test("a run that starts locked: nothing is captured during the unlock wait; the question gets the later screen")
    func unlockWait() async throws {
        // Starts locked; the driver waits for the unlock (no capture then), observes again,
        // and the question gets a frame of that later, unlocked observation.
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        phone.lock(forChecks: 3)
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store)
        #expect(run.updates.contains { if case .awaitingUnlock = $0 { true } else { false } })
        #expect(phone.capturesWhileLocked == 0)
        let evaluated = try #require(run.evaluated.first)
        #expect(gate.asked.first?.key == ViewerFrameKey(taskId: "ask", observation: evaluated))
        #expect(gate.asked.first?.frame == approvalPixels)
        #expect(phone.captures == 1)
        #expect(store.count == 0)
    }

    @Test("option off, or no store, or a store that does not accept: no capture call at all")
    func optionOff() async throws {
        let store = ApprovalFrameStore()
        for (option, used) in [(false, store as ApprovalFrameStore?), (true, nil)] {
            let phone = ApprovalPhone()
            let gate = ApprovalGate([.decline], store: store)
            let run = try await driveApproval(phone, gate: gate, store: used, option: option)
            #expect(phone.captures == 0)
            #expect(phone.snapshots == run.observationCount, "only the core's observations")
            #expect(gate.asked.first?.frame == nil)
        }
        let off = ApprovalFrameStore()
        off.isEnabled = false
        let phone = ApprovalPhone()
        let run = try await driveApproval(phone, gate: ApprovalGate([.decline], store: off), store: off)
        #expect(phone.captures == 0 && phone.snapshots == run.observationCount)
        let inactive = ApprovalFrameStore()
        inactive.isAppActive = false
        let background = ApprovalPhone()
        let backgroundRun = try await driveApproval(background, gate: ApprovalGate([.decline], store: inactive), store: inactive)
        #expect(background.captures == 0 && background.snapshots == backgroundRun.observationCount)
    }

    @Test("a capture that hangs ends at its deadline: the question is not held up and the run is the same")
    func hangingCapture() async throws {
        for reply: ApprovalGate.Reply in [.approve, .decline] {
            let store = ApprovalFrameStore()
            let phone = ApprovalPhone(.hangs)
            let gate = ApprovalGate([reply], store: store)
            let clock = ContinuousClock()
            let start = clock.now
            let on = try await driveApproval(phone, gate: gate, store: store, timeout: .milliseconds(60))
            #expect(clock.now - start < .seconds(10))
            let asked = try #require(gate.askedAt.first)
            #expect(asked - start < .seconds(5), "the question went out at the capture's deadline")
            #expect(gate.asked.first?.frame == nil)

            let offPhone = ApprovalPhone(.hangs)
            let off = try await driveApproval(offPhone, gate: ApprovalGate([reply], store: nil), store: nil, option: false)
            #expect(phone.commands == offPhone.commands, "the same commands with and without the option (\(reply))")
            #expect(on.result.outcome.status == off.result.outcome.status)
            #expect(on.result.effectCount == off.result.effectCount)
            #expect(!on.updates.contains { if case .ioFailed = $0 { true } else { false } })
            #expect(on.updates.count == off.updates.count, "no extra update, note or failure")
        }
        // A capture that works changes nothing either.
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let on = try await driveApproval(phone, gate: ApprovalGate([.approve], store: store), store: store)
        let offPhone = ApprovalPhone()
        let off = try await driveApproval(offPhone, gate: ApprovalGate([.approve], store: nil), store: nil, option: false)
        #expect(on.result.outcome.status == .success)
        #expect(phone.commands == offPhone.commands)
        #expect(on.result.effectCount == off.result.effectCount)
    }
}

// MARK: - The preview model (through the adapter, as the app feeds it)

private let phoneSize = Size(width: 393, height: 852)

private func anode(_ role: Role, _ label: String?, _ frame: Rect, focused: Bool = false, _ children: [UiNode] = []) -> UiNode {
    UiNode(role: role, label: label, value: nil, identifier: nil, placeholder: nil, frame: frame, enabled: true, visible: true,
           hittable: true, focused: focused, selected: false, secure: false, children: children)
}

private let payFrame = Rect(x: 16, y: 700, width: 361, height: 50)
private let noteFrame = Rect(x: 16, y: 200, width: 361, height: 44)

private func shopScreen() -> Screen {
    let full = Rect(x: 0, y: 0, width: phoneSize.width, height: phoneSize.height)
    let root = anode(.application, "Shop", full, [
        anode(.window, nil, full, [
            anode(.textField, "Note", noteFrame, focused: true),
            anode(.button, "Pay $512.40", payFrame),
        ]),
    ])
    return Screen(appBundleId: "com.example.shop", appName: "Shop", size: phoneSize, root: root, keyboardVisible: false,
                  orientation: .portrait)
}

private let payRequest = ConfirmRequest(category: .payment, summary: "Pay $512.40 to Example", app: "com.example.shop")

/// A run that observes the shop screen and asks to confirm `action`, fed through the adapter.
private func asked(_ action: Action, taskId: String = "shop") -> (LiveViewModel, CoreApprovalTarget?) {
    let viewer = LiveViewModel()
    var events: [RunEvent] = [.update(.started(TaskSpec(id: taskId, text: "Pay")))]
    events.append(.update(.effect(.observe(screenshot: false), number: 1)))
    events.append(.update(.effectCompleted(.observe(screenshot: false), event: .observed(observation: ScreenObservation(
        screen: shopScreen(), screenshot: nil, deviceLocked: false)))))
    events.append(.update(.effect(.confirm(request: payRequest), number: 2)))
    events.append(.update(.progress(SessionProgress(step: 1, phase: .acting, lastAction: action, app: "com.example.shop",
                                                    modelCalls: 1, parseRetries: 0, waitingForUnlock: false, lockedWaitMs: 0,
                                                    lockedTotalMs: 0))))
    events.append(.update(.confirmationPending(payRequest)))
    for e in events { viewer.apply(e) }
    return (viewer, viewer.approvalTarget)
}

/// The index the core gives the element labelled `label` on the shop screen (read from a
/// real confirmation's picture: tap each index until the outline lands on it).
private func index(of frame: Rect) -> UInt32? {
    (1...6).first { (i: UInt32) -> Bool in
        guard case .outline(let rect, _)? = asked(.tap(element: i)).1?.mark else { return false }
        return rect == frame
    }
}

private func unit(_ r: Rect) -> Rect {
    Rect(x: r.x / phoneSize.width, y: r.y / phoneSize.height, width: r.width / phoneSize.width, height: r.height / phoneSize.height)
}

private func close(_ a: Rect, _ b: Rect) -> Bool {
    abs(a.x - b.x) < 1e-9 && abs(a.y - b.y) < 1e-9 && abs(a.width - b.width) < 1e-9 && abs(a.height - b.height) < 1e-9
}

@Suite("Approval preview: the target on the evaluated screen", .timeLimit(.minutes(1)))
struct ApprovalTargetTests {
    private func preview(_ action: Action) throws -> ApprovalPreview {
        let (_, target) = asked(action)
        return try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: payRequest), target: target,
                                                 frames: ApprovalFrameStore(), appActive: true))
    }

    @Test("tap and long press outline the element; the label names it for VoiceOver")
    func tapAndLongPress() throws {
        let pay = try #require(index(of: payFrame))
        for action in [Action.tap(element: pay), .longPress(element: pay, durationMs: 800)] {
            let p = try preview(action)
            guard case .outline(let rect) = p.mark else {
                Issue.record("\(action): \(p.mark)")
                continue
            }
            #expect(close(rect, unit(payFrame)))
            #expect(p.accessibilityLabel == "Outlined: Button 'Pay $512.40'")
            #expect(p.backdrop == .layout && p.badge == "Layout only")
            #expect(p.key == ViewerFrameKey(taskId: "shop", observation: 1))
            #expect(p.screenSize == phoneSize)
        }
        #expect(try preview(.longPress(element: pay, durationMs: 800)).caption.contains("press and hold"))
    }

    @Test("type outlines the field (named, or the focused one)")
    func typing() throws {
        let note = try #require(index(of: noteFrame))
        for action in [Action.type(element: note, text: "hi", submit: false), .type(element: nil, text: "hi", submit: false)] {
            let p = try preview(action)
            guard case .outline(let rect) = p.mark else {
                Issue.record("\(action): \(p.mark)")
                continue
            }
            #expect(close(rect, unit(noteFrame)))
            #expect(p.caption.contains("type into"))
            #expect(p.accessibilityLabel == "Outlined: TextField 'Note'")
        }
    }

    @Test("tap_point marks the point; off the screen it marks nothing and the caption says so")
    func tapPoint() throws {
        let p = try preview(.tapPoint(x: 0.5, y: 0.8))
        guard case .point(let point) = p.mark else {
            Issue.record("\(p.mark)")
            return
        }
        #expect(abs(point.x - 0.5) < 1e-12 && abs(point.y - 0.8) < 1e-12)
        #expect(p.accessibilityLabel.hasPrefix("Marked: a point 50% across and 80% down"))
        for off in [Action.tapPoint(x: 1.4, y: 0.5), .tapPoint(x: -0.1, y: 0.2)] {
            let q = try preview(off)
            #expect(q.mark == .none)
            #expect(q.caption.contains("not on the visible screen"), "\(q.caption)")
        }
        let missing = try preview(.tap(element: 99))
        #expect(missing.mark == .none && missing.caption.contains("could not find"))
        let home = try preview(.pressHome)
        #expect(home.mark == .none && home.accessibilityLabel.hasPrefix("Nothing outlined."))
    }

    @Test("unit rectangles are placed in the drawn picture; nothing off it is placed")
    func placing() {
        #expect(ApprovalPreview.place(Rect(x: 0.25, y: 0.5, width: 0.5, height: 0.25), width: 200, height: 400)
            == Rect(x: 50, y: 200, width: 100, height: 100))
        #expect(ApprovalPreview.place(Rect(x: 1.2, y: 0.5, width: 0.5, height: 0.25), width: 200, height: 400) == nil)
        #expect(ApprovalPreview.place(Rect(x: 0.2, y: 0.5, width: 0, height: 0.25), width: 200, height: 400) == nil)
        #expect(ApprovalPreview.place(Rect(x: .nan, y: 0.5, width: 0.1, height: 0.25), width: 200, height: 400) == nil)
        let clipped = ApprovalPreview.place(Rect(x: 0.9, y: 0.9, width: 0.5, height: 0.5), width: 100, height: 100)
        #expect(clipped.map { close($0, Rect(x: 90, y: 90, width: 10, height: 10)) } == true, "clipped to the picture")
    }

    @Test("the picture goes with the question: cleared by the answer")
    func clearedByTheAnswer() throws {
        let (viewer, target) = asked(.tap(element: 1))
        #expect(target != nil)
        viewer.apply(.update(.effectCompleted(.confirm(request: payRequest), event: .userDecision(approved: true))))
        #expect(viewer.approvalTarget == nil)
    }
}

// MARK: - Nothing of it leaves the phone's sheet

@Suite("Approval preview: never streamed, encoded or recorded", .timeLimit(.minutes(1)), .serialized)
struct ApprovalPrivacyTests {
    private static func contains(_ haystack: Data, _ needle: Data) -> Bool {
        haystack.range(of: needle) != nil
    }

    @Test("the encoded timeline and the history hold no pixels, caption or outline")
    func notEncoded() async throws {
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let gate = ApprovalGate([.approve], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store)
        #expect(gate.asked.first?.frame == approvalPixels, "a frame was held while asked")
        let viewer = LiveViewModel(framesEnabled: true)
        var whileAsked: Data?
        var caption = ""
        for e in run.events {
            viewer.apply(e)
            if case .update(.confirmationPending) = e {
                whileAsked = viewer.encode()
                caption = viewer.approvalTarget?.caption ?? ""
            }
        }
        #expect(!caption.isEmpty)
        for bytes in [try #require(whileAsked), try #require(viewer.encode())] {
            #expect(!Self.contains(bytes, approvalPixels.data))
            #expect(!Self.contains(bytes, Data(caption.utf8)))
            #expect(!Self.contains(bytes, Data("Outlined".utf8)))
        }

        // Through the coordinator: the history file holds none of it either.
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("approval-history-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let frames = ApprovalFrameStore()
        let coordinator = JarvisCoordinator(
            paths: JarvisPaths(directory: dir), selection: .demo, agentBundleId: "dev.jarvis.app", tools: nil,
            configuration: DriverConfiguration(lockWatchInterval: nil), demoDelay: .zero, demoLockedPolls: 0,
            maxLockedWaitOverrideMs: nil, viewerFrames: ViewerFrameStore(), approvalFrames: frames,
            demoRunner: { _, _ in ApprovalPhone() }
        )
        let coordinatorGate = ApprovalGate([.approve], store: frames)
        var settings = JarvisSettings()
        settings.firstUseApproval = false
        #expect(settings.approvalScreenshots, "on by default")
        let outcome = await coordinator.run(task: "Delete my account", settings: settings, human: coordinatorGate, onEvent: { _ in })
        guard case .finished = outcome else {
            Issue.record("\(outcome)")
            return
        }
        #expect(coordinatorGate.asked.first?.frame == approvalPixels)
        #expect(frames.count == 0, "nothing held once the run ended")
        let history = try Data(contentsOf: coordinator.paths.history)
        #expect(!Self.contains(history, approvalPixels.data))
        #expect(!Self.contains(history, Data("Outlined".utf8)))
        let written = try FileManager.default.contentsOfDirectory(atPath: dir.path)
        for name in written {
            let bytes = (try? Data(contentsOf: dir.appendingPathComponent(name))) ?? Data()
            #expect(!Self.contains(bytes, approvalPixels.data), "\(name)")
        }

        // The option off in the settings: no capture at all.
        let offFrames = ApprovalFrameStore()
        let offPhone = ApprovalPhone()
        let offCoordinator = JarvisCoordinator(
            paths: JarvisPaths(directory: dir), selection: .demo, agentBundleId: "dev.jarvis.app", tools: nil,
            configuration: DriverConfiguration(lockWatchInterval: nil), demoDelay: .zero, demoLockedPolls: 0,
            maxLockedWaitOverrideMs: nil, viewerFrames: ViewerFrameStore(), approvalFrames: offFrames,
            demoRunner: { _, _ in offPhone }
        )
        settings.approvalScreenshots = false
        _ = await offCoordinator.run(task: "Delete my account", settings: settings,
                                     human: ApprovalGate([.approve], store: offFrames), onEvent: { _ in })
        #expect(offPhone.captures == 0)
    }

    @Test("a paired device gets no image, caption or outline: remote approval stays text only")
    @MainActor
    func notStreamed() async throws {
        let phone = ApprovalPhone()
        let r = RemoteHarness("approval-remote", demoRunner: { _, _ in phone })
        defer { r.cleanUp() }
        let remote = try await r.pairDevice("Test Mac")
        try remote.send(.subscribe)
        try await Task.sleep(for: .milliseconds(50))
        let mark = remote.messages.count
        try remote.send(.startTask(text: "Delete my account"))
        let pending = try await remote.waitForMessage(after: mark) { m in
            if case .confirmPending(_, .delete, _, _) = m { return true }
            return false
        }
        // The phone's own sheet has the frame while the question is open.
        #expect(r.base.coordinator.approvalFrames.frame(for: try #require(r.base.coordinator.approvalFrames.key)) == approvalPixels)
        guard case .confirmPending(let requestId, _, _, _) = pending else { return }
        try remote.send(.decision(requestId: requestId, approved: false))
        _ = try await remote.waitForMessage(after: mark) { m in
            if case .finished = m { return true }
            return false
        }
        try await eventually("the session to end") { r.base.center.session == nil }
        #expect(r.base.coordinator.approvalFrames.count == 0)
        let text = remote.messages.map { String(describing: $0) }.joined(separator: "\n")
        for fragment in ["Outlined", "What Jarvis sees", "Layout only", "Jarvis will tap", "outlined"] {
            #expect(!text.contains(fragment), "\(fragment)")
        }
        let sent = r.log.all.compactMap { if case .send(_, let bytes) = $0 { bytes } else { nil } }
        #expect(!sent.isEmpty)
        for bytes in sent {
            #expect(!Self.contains(bytes, approvalPixels.data))
        }
    }
}

@Suite("Approval preview: the setting", .timeLimit(.minutes(1)))
struct ApprovalSettingTests {
    private func decode(_ json: String) throws -> JarvisSettings {
        try JSONDecoder().decode(JarvisSettings.self, from: Data(json.utf8))
    }

    @Test("on by default, separate from the Live view's option, lenient when unreadable")
    func defaults() throws {
        #expect(JarvisSettings().approvalScreenshots)
        #expect(!JarvisSettings().liveViewScreenshots)
        #expect(try decode("{}").approvalScreenshots)
        #expect(try !decode(#"{"approval_screenshots": false}"#).approvalScreenshots)
        #expect(try decode(#"{"approval_screenshots": "nope"}"#).approvalScreenshots)
        #expect(try !decode(#"{"approval_screenshots": true}"#).liveViewScreenshots)
        var settings = JarvisSettings()
        settings.approvalScreenshots = false
        let data = try JSONEncoder().encode(settings)
        #expect(try !JSONDecoder().decode(JarvisSettings.self, from: data).approvalScreenshots)
        #expect(!DriverConfiguration().approvalScreenshots, "a driver built without the option captures nothing")
    }
}
