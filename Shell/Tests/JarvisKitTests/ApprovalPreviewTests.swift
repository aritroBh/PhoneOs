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

/// Twenty-fourth review (F2): a model that asks for a `look` first (a vision model: the
/// core takes a screenshot with the tree), then plays the script. The question that follows
/// is computed from that observation, which carries the core's pixels — the only pixels the
/// approval sheet may show.
private final class LookFirstModel: ModelProvider {
    let inner: ScriptedModel
    private let looked = Mutex(false)

    init(_ inner: ScriptedModel) { self.inner = inner }

    var capabilities: ModelCapabilities { inner.capabilities }

    func generate(_ request: ModelRequest) async throws -> String {
        let first = looked.withLock { l -> Bool in
            defer { l = true }
            return !l
        }
        if first { return #"{"thought":"look","action":"look"}"# }
        return try await inner.generate(request)
    }
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
    viewerFrames: ViewerFrameStore? = nil, vision: Bool = false, cancelAfter: Duration? = nil,
    lookFirst: Bool = false
) async throws -> ApprovalRun {
    let scripted = ScriptedModel(steps: DemoPhone.deleteAccountScript,
                                 capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: vision || lookFirst))
    let model: any ModelProvider = lookFirst ? LookFirstModel(scripted) : scripted
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
        // Twenty-fourth review (F2): the frame is the pixels the core asked for with the
        // evaluated observation (a `look` before the question); was: a capture at question
        // time.
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store, lookFirst: true)
        let evaluated = try #require(run.evaluated.first)
        #expect(gate.asked.count == 1)
        #expect(gate.asked.first?.key == ViewerFrameKey(taskId: "ask", observation: evaluated))
        #expect(gate.asked.first?.frame == corePixels)
        #expect(phone.captures == 0, "no capture for the question")
        #expect(phone.snapshots == run.observationCount, "only the core's observations")
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
        // Preview review: the badges say what the picture is ("Screen when Jarvis asked",
        // "Layout from the app's data"; were "What Jarvis sees now", "Layout only").
        #expect(real.backdrop == .realScreen && real.badge == "Screen when Jarvis asked")
        #expect(real.frame(in: store) == approvalPixels)
        #expect(ApprovalPreview.make(prompt: prompt, target: target, frames: store, appActive: false)?.backdrop == .layout,
                "never the real screen while the app is not in front")
        #expect(store.count == 1, "but the frame is held while Jarvis is in the background")
        store.put(approvalPixels, for: ViewerFrameKey(taskId: "ask", observation: evaluated - 1))
        let other = try #require(ApprovalPreview.make(prompt: prompt, target: target, frames: store, appActive: true))
        #expect(other.backdrop == .layout && other.badge == "Layout from the app's data",
                "a frame of another observation is never shown")
        #expect(other.frame(in: store) == nil)
        // Only for the confirmation the picture belongs to.
        let wrong = ConfirmRequest(category: target.request.category, summary: "Something else", app: target.request.app, phoneOnly: false)
        #expect(ApprovalPreview.make(prompt: .confirm(id: 7, request: wrong), target: target, frames: store, appActive: true) == nil)
        #expect(ApprovalPreview.make(prompt: .question(id: 7, text: "?"), target: target, frames: store, appActive: true) == nil)
        #expect(real.isLive(pending: prompt) && !real.isLive(pending: .confirm(id: 8, request: target.request)))
        #expect(!real.isLive(pending: nil))
    }

    @Test("twenty-fourth review (F2): no tree read and no capture between a question and its answer, option on or off")
    func nothingReadWhileAsked() async throws {
        for option in [true, false] {
            for lookFirst in [false, true] {
                for reply: ApprovalGate.Reply in [.approve, .decline] {
                    let store = ApprovalFrameStore()
                    let phone = ApprovalPhone()
                    let gate = ApprovalGate([reply], store: store)
                    let run = try await driveApproval(phone, gate: gate, store: store, option: option,
                                                      lockWatch: .milliseconds(20), lookFirst: lookFirst)
                    let what = "option \(option), look first \(lookFirst), \(reply)"
                    #expect(gate.asked.count == 1, "\(what)")
                    // Every tree read is one of the core's observations: none at question time.
                    #expect(phone.snapshots == run.observationCount, "\(what)")
                    #expect(phone.captures == 0, "\(what)")
                    let pixels: CapturedScreenshot? = option && lookFirst ? corePixels : nil
                    #expect(gate.asked.first?.frame == pixels, "\(what)")
                    #expect(store.count == 0, "\(what)")
                    // The preview still has its wireframe, from the observation the core decided on.
                    let viewer = LiveViewModel()
                    for e in run.events {
                        viewer.apply(e)
                        if case .update(.confirmationPending) = e { break }
                    }
                    let target = try #require(viewer.approvalTarget, "\(what)")
                    let evaluated = try #require(run.evaluated.first)
                    #expect(ViewerFrameKey(target.frameKey) == ViewerFrameKey(taskId: "ask", observation: evaluated), "\(what)")
                    let preview = try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: target.request),
                                                                    target: target, frames: ApprovalFrameStore(),
                                                                    appActive: true), "\(what)")
                    #expect(preview.backdrop == .layout, "\(what)")
                    let screen = try #require(viewer.screen(id: preview.screenId), "\(what)")
                    #expect(!screen.elements.isEmpty, "\(what): the wireframe of the evaluated observation")
                }
            }
        }
    }

    @Test("a screen that moves on after it was evaluated is not read again: the frame is the evaluated observation's own")
    func movedOn() async throws {
        // Twenty-fourth review (F2): was "no frame" after a re-read showed the moved screen;
        // nothing is re-read now, and the core's look after an approval protects the action.
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone(moveOnAfterCapture: true)
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store, lookFirst: true)
        #expect(phone.captures == 0 && phone.snapshots == run.observationCount)
        #expect(gate.asked.count == 1 && gate.asked.first?.frame == corePixels)
        #expect(store.count == 0)
    }

    @Test("only pixels the core asked for with that very observation; never the Live view's frame of it")
    func reused() async throws {
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store, vision: true)
        let evaluated = try #require(run.evaluated.first)
        // Whether the core asked pixels of the evaluated observation.
        var asked = false
        for u in run.updates {
            if case .effect(let effect, let n) = u, n == evaluated, case .observe(let s) = effect { asked = s }
        }
        #expect(gate.asked.first?.frame == (asked ? corePixels : nil), "the core's pixels of the evaluated screen, or none")
        #expect(phone.captures == 0, "no capture for the question")

        // Twenty-fourth review (F2): the Live view's frame (taken after the evaluated tree
        // read) is not shown (was: shown after one more tree read equalled the evaluated tree).
        // The Live view's own best-effort captures are unchanged: one per observation.
        let frames = ViewerFrameStore()
        let livePhone = ApprovalPhone()
        let liveGate = ApprovalGate([.decline], store: store)
        let liveRun = try await driveApproval(livePhone, gate: liveGate, store: store, viewerFrames: frames)
        #expect(liveRun.evaluated.count == 1)
        #expect(liveGate.asked.first?.key == nil && liveGate.asked.first?.frame == nil, "no frame held")
        #expect(livePhone.captures == liveRun.observationCount, "the Live view's captures only, none for the question")
        #expect(livePhone.snapshots == liveRun.observationCount, "no tree re-read for the question")
        #expect(store.count == 0)
    }

    @Test("dropped on every resolution: approved, declined, failed, timed out, cancelled, a lock")
    func droppedOnEveryResolution() async throws {
        for reply: ApprovalGate.Reply in [.approve, .decline, .fail] {
            let store = ApprovalFrameStore()
            let gate = ApprovalGate([reply], store: store)
            _ = try await driveApproval(ApprovalPhone(), gate: gate, store: store, lookFirst: true)
            #expect(gate.asked.first?.frame == corePixels, "\(reply)")
            #expect(store.count == 0, "\(reply)")
        }
        // Timed out.
        let timedStore = ApprovalFrameStore()
        let timedGate = ApprovalGate([.hang], store: timedStore)
        _ = try await driveApproval(ApprovalPhone(), gate: timedGate, store: timedStore, humanTimeout: .milliseconds(80),
                                    lookFirst: true)
        #expect(timedGate.asked.first?.frame == corePixels)
        #expect(timedStore.count == 0)
        // The run cancelled while the owner is asked.
        let cancelStore = ApprovalFrameStore()
        let cancelGate = ApprovalGate([.hang], store: cancelStore)
        let cancelled = try await driveApproval(ApprovalPhone(), gate: cancelGate, store: cancelStore,
                                                cancelAfter: .milliseconds(300), lookFirst: true)
        #expect(cancelled.result.endReason == .cancelled)
        #expect(cancelGate.asked.count == 1 && cancelGate.asked.first?.frame == corePixels)
        #expect(cancelStore.count == 0)
        // The phone locks while the owner is asked: the question is abandoned, then asked
        // again after the unlock, and its frame is dropped too. Twenty-fourth review: the
        // question asked again is computed from the observation after the unlock, which
        // carries no pixels the core asked for: no frame (was: a fresh capture).
        let lockStore = ApprovalFrameStore()
        let lockPhone = ApprovalPhone()
        let lockGate = ApprovalGate([.hang, .decline], store: lockStore, onAsk: { n in if n == 1 { lockPhone.lock(forChecks: 2) } })
        let locked = try await driveApproval(lockPhone, gate: lockGate, store: lockStore, lockWatch: .milliseconds(20),
                                             lookFirst: true)
        #expect(locked.updates.contains { if case .lockDetected(.confirm) = $0 { true } else { false } })
        #expect(lockGate.asked.count == 2)
        #expect(lockGate.asked.first?.frame == corePixels)
        #expect(lockGate.asked.last?.frame == nil)
        #expect(lockPhone.captures == 0, "never captured")
        #expect(lockStore.count == 0)
        // Preview review (new rule): the phone locked (protected data unavailable) while the
        // sheet was up drops the frame and refuses new ones; Jarvis merely being in the
        // background does not (that was `isAppActive = false`, which dropped it).
        let appStore = ApprovalFrameStore()
        appStore.put(approvalPixels, for: ViewerFrameKey(taskId: "t", observation: 1))
        appStore.isPhoneUnlocked = false
        #expect(appStore.count == 0 && !appStore.accepts)
        #expect(!appStore.put(approvalPixels, for: ViewerFrameKey(taskId: "t", observation: 1)))
        appStore.isPhoneUnlocked = true
        #expect(appStore.put(approvalPixels, for: ViewerFrameKey(taskId: "t", observation: 1)))
        appStore.remove(ViewerFrameKey(taskId: "t", observation: 2))
        #expect(appStore.count == 1, "removing another observation's key leaves the frame")
        appStore.isEnabled = false
        #expect(appStore.count == 0)
    }

    @Test("never kept unless the evaluated observation was known unlocked")
    func lockRules() async throws {
        let unknownStore = ApprovalFrameStore()
        let unknownPhone = ApprovalPhone(reportedLock: .unknown)
        let unknownGate = ApprovalGate([.decline], store: unknownStore)
        let unknown = try await driveApproval(unknownPhone, gate: unknownGate, store: unknownStore, lookFirst: true)
        #expect(unknownPhone.captures == 0, "a runner that cannot tell its lock state gets no capture")
        #expect(unknownPhone.snapshots == unknown.observationCount, "and no extra tree read")
        #expect(unknownGate.asked.first?.frame == nil, "the core's pixels of a screen not known unlocked are not kept")
    }

    @Test("a run that starts locked: nothing is captured during the unlock wait; the question gets the later screen")
    func unlockWait() async throws {
        // Starts locked; the driver waits for the unlock (no capture then), observes again,
        // and the question gets the frame of that later, unlocked observation (the core's
        // `look` pixels, twenty-fourth review).
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        phone.lock(forChecks: 3)
        let gate = ApprovalGate([.decline], store: store)
        let run = try await driveApproval(phone, gate: gate, store: store, lookFirst: true)
        #expect(run.updates.contains { if case .awaitingUnlock = $0 { true } else { false } })
        #expect(phone.capturesWhileLocked == 0)
        let evaluated = try #require(run.evaluated.first)
        #expect(gate.asked.first?.key == ViewerFrameKey(taskId: "ask", observation: evaluated))
        #expect(gate.asked.first?.frame == corePixels)
        #expect(phone.captures == 0)
        #expect(store.count == 0)
    }

    @Test("option off, or no store, or a store that does not accept: no capture call at all")
    func optionOff() async throws {
        let store = ApprovalFrameStore()
        for (option, used) in [(false, store as ApprovalFrameStore?), (true, nil)] {
            let phone = ApprovalPhone()
            let gate = ApprovalGate([.decline], store: store)
            let run = try await driveApproval(phone, gate: gate, store: used, option: option, lookFirst: true)
            #expect(phone.captures == 0)
            #expect(phone.snapshots == run.observationCount, "only the core's observations")
            #expect(gate.asked.first?.frame == nil)
        }
        let off = ApprovalFrameStore()
        off.isEnabled = false
        let phone = ApprovalPhone()
        let run = try await driveApproval(phone, gate: ApprovalGate([.decline], store: off), store: off, lookFirst: true)
        #expect(phone.captures == 0 && phone.snapshots == run.observationCount)
        // Preview review (new rule): the phone locked (protected data unavailable) refuses
        // frames; that was `isAppActive = false`, which no longer exists: whether Jarvis is
        // in front does not matter (the target app is in front when it asks).
        let locked = ApprovalFrameStore()
        locked.isPhoneUnlocked = false
        let background = ApprovalPhone()
        let backgroundRun = try await driveApproval(background, gate: ApprovalGate([.decline], store: locked), store: locked)
        #expect(background.captures == 0 && background.snapshots == backgroundRun.observationCount)
    }

    @Test("a runner whose capture would hang is never asked for one: the question is not held up and the run is the same")
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
            #expect(asked - start < .seconds(5), "the question went out at once")
            #expect(gate.asked.first?.frame == nil)
            #expect(phone.captures == 0)

            let offPhone = ApprovalPhone(.hangs)
            let off = try await driveApproval(offPhone, gate: ApprovalGate([reply], store: nil), store: nil, option: false)
            #expect(phone.commands == offPhone.commands, "the same commands with and without the option (\(reply))")
            #expect(on.result.outcome.status == off.result.outcome.status)
            #expect(on.result.effectCount == off.result.effectCount)
            #expect(!on.updates.contains { if case .ioFailed = $0 { true } else { false } })
            #expect(on.updates.count == off.updates.count, "no extra update, note or failure")
        }
        // A frame kept changes nothing either.
        let store = ApprovalFrameStore()
        let phone = ApprovalPhone()
        let on = try await driveApproval(phone, gate: ApprovalGate([.approve], store: store), store: store, lookFirst: true)
        let offPhone = ApprovalPhone()
        let off = try await driveApproval(offPhone, gate: ApprovalGate([.approve], store: nil), store: nil, option: false,
                                          lookFirst: true)
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

private let payRequest = ConfirmRequest(category: .payment, summary: "Pay $512.40 to Example", app: "com.example.shop", phoneOnly: false)

/// The request the core sends when it asks about an action of kind `verb` on the shop screen:
/// the target it resolved (the element with its frame, or the point in screen points) on that
/// screen's digest; no target when neither is given. What the picture marks comes from this,
/// never from the step history.
private func stated(_ verb: ConfirmVerb, element: UInt32? = nil, frame: Rect? = nil, point: Point? = nil) -> ConfirmRequest {
    let digest = (try? screenDigestHex(screen: shopScreen())) ?? ""
    let target: ConfirmTarget? = element == nil && point == nil
        ? nil
        : ConfirmTarget(element: element, node: nil, frame: frame, point: point, digest: digest)
    return ConfirmRequest(category: payRequest.category, summary: payRequest.summary, app: payRequest.app, phoneOnly: false,
                          rows: [], stage: .whole, verb: verb, target: target)
}

/// A run that observes the shop screen and asks to confirm `action` (with `request`), fed
/// through the adapter.
private func asked(_ action: Action, taskId: String = "shop", request: ConfirmRequest = payRequest) -> (LiveViewModel, CoreApprovalTarget?) {
    let viewer = LiveViewModel()
    var events: [RunEvent] = [.update(.started(TaskSpec(id: taskId, text: "Pay")))]
    events.append(.update(.effect(.observe(screenshot: false), number: 1)))
    events.append(.update(.effectCompleted(.observe(screenshot: false), event: .observed(observation: ScreenObservation(
        screen: shopScreen(), screenshot: nil, deviceLocked: false)))))
    events.append(.update(.effect(.confirm(request: request), number: 2)))
    events.append(.update(.progress(SessionProgress(step: 1, phase: .acting, lastAction: action, app: "com.example.shop",
                                                    modelCalls: 1, parseRetries: 0, waitingForUnlock: false, lockedWaitMs: 0,
                                                    lockedTotalMs: 0))))
    events.append(.update(.confirmationPending(request)))
    for e in events { viewer.apply(e) }
    return (viewer, viewer.approvalTarget)
}

/// The index the core gives the element with `frame` on the shop screen (read from the
/// picture: state each index with that frame until the outline lands on it; the viewer marks
/// an element only when its frame is the stated one).
private func index(of frame: Rect) -> UInt32? {
    (1...6).first { (i: UInt32) -> Bool in
        let request = stated(.tap, element: i, frame: frame)
        guard case .outline(let rect, _)? = asked(.tap(element: i), request: request).1?.mark else { return false }
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
    private func preview(_ action: Action, _ request: ConfirmRequest) throws -> ApprovalPreview {
        let (_, target) = asked(action, request: request)
        return try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: request), target: target,
                                                 frames: ApprovalFrameStore(), appActive: true))
    }

    @Test("tap and long press outline the element; the label names it for VoiceOver")
    func tapAndLongPress() throws {
        let pay = try #require(index(of: payFrame))
        for (action, verb) in [(Action.tap(element: pay), ConfirmVerb.tap), (.longPress(element: pay, durationMs: 800), .longPress)] {
            let p = try preview(action, stated(verb, element: pay, frame: payFrame))
            guard case .outline(let rect) = p.mark else {
                Issue.record("\(action): \(p.mark)")
                continue
            }
            #expect(close(rect, unit(payFrame)))
            #expect(p.accessibilityLabel == "Outlined: Button 'Pay $512.40'")
            // Preview review: was "Layout only".
            #expect(p.backdrop == .layout && p.badge == "Layout from the app's data")
            #expect(p.key == ViewerFrameKey(taskId: "shop", observation: 1))
            #expect(p.screenSize == phoneSize)
        }
        #expect(try preview(.longPress(element: pay, durationMs: 800),
                            stated(.longPress, element: pay, frame: payFrame)).caption.contains("press and hold"))
    }

    @Test("type outlines the field (named, or the focused one)")
    func typing() throws {
        let note = try #require(index(of: noteFrame))
        // Without an element the core resolves the focused field, and states it.
        for action in [Action.type(element: note, text: "hi", submit: false), .type(element: nil, text: "hi", submit: false)] {
            let p = try preview(action, stated(.type, element: note, frame: noteFrame))
            guard case .outline(let rect) = p.mark else {
                Issue.record("\(action): \(p.mark)")
                continue
            }
            #expect(close(rect, unit(noteFrame)))
            // Preview review: "The outline is where Jarvis will type. …" (was "… type into …").
            #expect(p.caption.hasPrefix("The outline is where Jarvis will type."))
            #expect(p.accessibilityLabel == "Outlined: TextField 'Note'")
        }
    }

    @Test("tap_point marks the point; off the screen it marks nothing and the caption says so")
    func tapPoint() throws {
        let p = try preview(.tapPoint(x: 0.5, y: 0.8),
                            stated(.tapPoint, point: Point(x: 0.5 * phoneSize.width, y: 0.8 * phoneSize.height)))
        guard case .point(let point) = p.mark else {
            Issue.record("\(p.mark)")
            return
        }
        #expect(abs(point.x - 0.5) < 1e-12 && abs(point.y - 0.8) < 1e-12)
        #expect(p.accessibilityLabel.hasPrefix("Marked: a point 50% across and 80% down"))
        for (x, y) in [(1.4, 0.5), (-0.1, 0.2)] {
            let q = try preview(.tapPoint(x: x, y: y),
                                stated(.tapPoint, point: Point(x: x * phoneSize.width, y: y * phoneSize.height)))
            #expect(q.mark == .none)
            #expect(q.caption.contains("not on the visible screen"), "\(q.caption)")
        }
        let missing = try preview(.tap(element: 99), stated(.tap, element: 99))
        #expect(missing.mark == .none && missing.caption.contains("could not find"))
        let home = try preview(.pressHome, stated(.pressHome))
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
        let run = try await driveApproval(phone, gate: gate, store: store, lookFirst: true)
        #expect(gate.asked.first?.frame == corePixels, "a frame was held while asked")
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
            #expect(!Self.contains(bytes, corePixels.data))
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
        // Twenty-fourth review (F2): the demo's question follows an observation without the
        // core's pixels: no frame (was: a capture at question time).
        #expect(coordinatorGate.asked.first?.frame == nil)
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
        // The phone's own sheet names the open question; twenty-fourth review (F2): no frame
        // is captured for it (the demo's observation carries no pixels the core asked for).
        #expect(r.base.coordinator.approvalFrames.questionKey != nil)
        #expect(r.base.coordinator.approvalFrames.count == 0)
        guard case .confirmPending(let requestId, _, _, _) = pending else { return }
        try remote.send(.decision(requestId: requestId, approved: false))
        _ = try await remote.waitForMessage(after: mark) { m in
            if case .finished = m { return true }
            return false
        }
        try await eventually("the session to end") { r.base.center.session == nil }
        #expect(r.base.coordinator.approvalFrames.count == 0)
        let text = remote.messages.map { String(describing: $0) }.joined(separator: "\n")
        for fragment in ["Outlined", "What Jarvis sees", "Layout only", "Jarvis will tap", "outlined",
                         "Screen when Jarvis asked", "Layout from", "No picture of this screen", "The outline is where",
                         "from when Jarvis asked"] {
            #expect(!text.contains(fragment), "\(fragment)")
        }
        let sent = r.log.all.compactMap { if case .send(_, let bytes) = $0 { bytes } else { nil } }
        #expect(!sent.isEmpty)
        for bytes in sent {
            #expect(!Self.contains(bytes, approvalPixels.data))
        }
    }
}

// MARK: - Preview review: what the picture may claim

private func shot(_ width: UInt32, _ height: UInt32) -> CapturedScreenshot {
    CapturedScreenshot(data: Data(repeating: 0x42, count: 24), mimeType: "image/png", pixelWidth: width, pixelHeight: height)
}

/// A run that observes the shop screen as runner text and asks to confirm `action`.
private func askedOnRunnerText(_ action: Action) -> (LiveViewModel, CoreApprovalTarget?) {
    let viewer = LiveViewModel()
    let text = """
    Application, label: 'Shop', {{0, 0}, {393, 852}}
      Window, {{0, 0}, {393, 852}}
        TextField, label: 'Note', value: 'private note text', {{16, 200}, {361, 44}}
        Button, label: 'Pay $512.40', {{16, 700}, {361, 50}}
    """
    let snapshot = RunnerSnapshot(source: text, appBundleId: "com.example.shop", appName: "Shop", size: phoneSize,
                                  keyboardVisible: false, orientation: .portrait, screenshot: nil, deviceLocked: false)
    var events: [RunEvent] = [.update(.started(TaskSpec(id: "text", text: "Pay")))]
    events.append(.update(.effect(.observe(screenshot: false), number: 1)))
    events.append(.update(.effectCompleted(.observe(screenshot: false), event: .observedRunnerText(snapshot: snapshot))))
    events.append(.update(.effect(.confirm(request: payRequest), number: 2)))
    events.append(.update(.progress(SessionProgress(step: 1, phase: .acting, lastAction: action, app: "com.example.shop",
                                                    modelCalls: 1, parseRetries: 0, waitingForUnlock: false, lockedWaitMs: 0,
                                                    lockedTotalMs: 0))))
    events.append(.update(.confirmationPending(payRequest)))
    for e in events { viewer.apply(e) }
    return (viewer, viewer.approvalTarget)
}

@Suite("Approval preview: what the picture may claim (preview review)", .timeLimit(.minutes(1)))
struct ApprovalClaimTests {
    @Test("a frame is drawn as the screen only when its aspect ratio is the screen's (within 2%)")
    func aspect() throws {
        let (_, target) = asked(.tap(element: 1))
        let t = try #require(target)
        let key = try #require(ViewerFrameKey(t.frameKey))
        let prompt = HumanPrompt.confirm(id: 1, request: payRequest)
        for (frame, real) in [(shot(393, 852), true), (shot(1179, 2556), true), (shot(400, 852), true),
                              (shot(410, 852), false), (shot(852, 393), false), (shot(0, 0), false), (shot(393, 0), false)] {
            let store = ApprovalFrameStore()
            store.put(frame, for: key)
            let p = try #require(ApprovalPreview.make(prompt: prompt, target: t, frames: store, appActive: true))
            #expect((p.backdrop == .realScreen) == real, "\(frame.pixelWidth)x\(frame.pixelHeight)")
            #expect((p.frame(in: store) != nil) == real)
        }
    }

    @Test("bound to the open question: a picture of another observation with the same request text is not shown")
    func boundToTheOpenQuestion() throws {
        let (_, target) = asked(.tap(element: 1))
        let t = try #require(target)
        let key = try #require(ViewerFrameKey(t.frameKey))
        let prompt = HumanPrompt.confirm(id: 1, request: payRequest)
        let store = ApprovalFrameStore()
        store.beginQuestion(ViewerFrameKey(taskId: key.taskId, observation: key.observation + 4))
        #expect(ApprovalPreview.make(prompt: prompt, target: t, frames: store, appActive: true) == nil,
                "a stale picture with the same request is not this question's")
        store.beginQuestion(key)
        #expect(ApprovalPreview.make(prompt: prompt, target: t, frames: store, appActive: true) != nil)
        store.put(shot(393, 852), for: key)
        store.endQuestion(key)
        #expect(store.questionKey == nil && store.count == 0, "the question and its frame go together")
        // The question key holds no pixels: kept with the option off.
        store.isEnabled = false
        store.beginQuestion(key)
        #expect(store.questionKey == key && store.count == 0)
        store.removeAll()
        #expect(store.questionKey == nil)
    }

    @Test("every frame kept gets a new sequence number (an image cache keys on it)")
    func sequence() throws {
        let store = ApprovalFrameStore()
        let key = ViewerFrameKey(taskId: "t", observation: 3)
        store.put(shot(393, 852), for: key)
        let first = try #require(store.held(for: key))
        store.put(shot(393, 852), for: key)
        let second = try #require(store.held(for: key))
        #expect(first.sequence != second.sequence)
        #expect(store.held(for: ViewerFrameKey(taskId: "t", observation: 4)) == nil)
    }

    @Test("the badge says what is drawn: the screenshot, the app's layout, the runner's text, or no picture")
    func badges() throws {
        let (viewer, target) = asked(.tap(element: 1))
        let t = try #require(target)
        let p = try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: payRequest), target: t,
                                                  frames: ApprovalFrameStore(), appActive: true))
        let typed = viewer.screen(id: p.screenId)
        #expect(typed?.info.fromText == false)
        #expect(p.badge(screen: typed) == "Layout from the app's data")
        #expect(p.badge(screen: nil) == "No picture of this screen")

        let (textViewer, textTarget) = askedOnRunnerText(.tap(element: 2))
        let tt = try #require(textTarget)
        let q = try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: payRequest), target: tt,
                                                  frames: ApprovalFrameStore(), appActive: true))
        let text = try #require(textViewer.screen(id: q.screenId))
        #expect(text.info.fromText && text.info.source == .runnerText)
        #expect(!text.elements.isEmpty, "runner text has a wireframe now")
        #expect(q.badge(screen: text) == "Layout from the runner's text")
        let encoded = try #require(textViewer.encode())
        #expect(encoded.range(of: Data("private note text".utf8)) == nil, "a value never reaches the encoding")

        let store = ApprovalFrameStore()
        let key = try #require(ViewerFrameKey(t.frameKey))
        store.put(shot(393, 852), for: key)
        let real = try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: payRequest), target: t,
                                                     frames: store, appActive: true))
        #expect(real.badge(screen: typed) == "Screen when Jarvis asked")
    }

    @Test("tap_point names no element; the caption says the picture is from when Jarvis asked")
    func tapPointNamesNothing() throws {
        let (_, target) = asked(.tapPoint(x: 0.5, y: (700 + 25) / phoneSize.height),
                                request: stated(.tapPoint, point: Point(x: 0.5 * phoneSize.width, y: 700 + 25)))
        let t = try #require(target)
        #expect(!t.accessibilityLabel.contains("Pay"), "\(t.accessibilityLabel)")
        #expect(t.accessibilityLabel.contains("at the marked point"))
        #expect(t.caption == "The mark is where Jarvis will tap. The picture is from when Jarvis asked and may have changed.")
        let one = try #require([noteFrame, payFrame].first { index(of: $0) == 1 })
        let (_, tap) = asked(.tap(element: 1), request: stated(.tap, element: 1, frame: one))
        #expect(try #require(tap).caption.hasSuffix("The picture is from when Jarvis asked and may have changed."))
    }

    @Test("a failed feed drops the approval picture")
    func failedFeed() throws {
        let inner = try SwitchableViewer()
        let viewer = LiveViewModel(viewer: inner)
        let (_, reference) = asked(.tap(element: 1))
        #expect(reference != nil)
        var events: [RunEvent] = [.update(.started(TaskSpec(id: "shop", text: "Pay")))]
        events.append(.update(.effect(.observe(screenshot: false), number: 1)))
        events.append(.update(.effectCompleted(.observe(screenshot: false), event: .observed(observation: ScreenObservation(
            screen: shopScreen(), screenshot: nil, deviceLocked: false)))))
        events.append(.update(.effect(.confirm(request: payRequest), number: 2)))
        events.append(.update(.progress(SessionProgress(step: 1, phase: .acting, lastAction: .tap(element: 1),
                                                        app: "com.example.shop", modelCalls: 1, parseRetries: 0,
                                                        waitingForUnlock: false, lockedWaitMs: 0, lockedTotalMs: 0))))
        events.append(.update(.confirmationPending(payRequest)))
        for e in events { viewer.apply(e) }
        #expect(viewer.approvalTarget != nil)
        inner.failing = true
        #expect(viewer.apply(.update(.questionPending("?"))), "the dropped picture is a change")
        #expect(viewer.approvalTarget == nil, "a picture of an earlier question is not kept after a failed feed")
    }
}

/// A real viewer that throws from every call once `failing` is set.
private final class SwitchableViewer: JarvisViewerProtocol, Sendable {
    private let inner: JarvisViewer
    private let broken = Mutex(false)

    init() throws {
        inner = try JarvisViewer(config: CoreViewerConfig(maxSteps: LiveViewModel.maxSteps, framesEnabled: false,
                                                          knownApps: LiveViewModel.knownApps))
    }

    var failing: Bool {
        get { broken.withLock { $0 } }
        set { broken.withLock { $0 = newValue } }
    }

    private func check() throws {
        if failing { throw JarvisError.Internal(message: "injected") }
    }

    func encode() throws -> Data { try check(); return try inner.encode() }
    func feed(update: CoreViewerUpdate) throws -> CoreViewerChange { try check(); return try inner.feed(update: update) }
    func generation() throws -> UInt64 { try check(); return try inner.generation() }
    func replay(action: CoreReplayAction, nowMs: Double, reduceMotion: Bool) throws -> CoreReplayStatus {
        try check()
        return try inner.replay(action: action, nowMs: nowMs, reduceMotion: reduceMotion)
    }
    func replayStatus() throws -> CoreReplayStatus { try check(); return try inner.replayStatus() }
    func scene(query: CoreSceneQuery) throws -> CoreLiveScene { try check(); return try inner.scene(query: query) }
    func screen(id: UInt32) throws -> CoreViewerScreen? { try check(); return try inner.screen(id: id) }
    func timeline() throws -> CoreViewerTimeline { try check(); return try inner.timeline() }
    func approvalTarget() throws -> CoreApprovalTarget? { try check(); return try inner.approvalTarget() }
    func feedAndStream(update: CoreViewerUpdate, hub: JarvisRemoteHub, nowMs: UInt64) throws -> CoreViewerFed {
        try check()
        return try inner.feedAndStream(update: update, hub: hub, nowMs: nowMs)
    }
    func applyDelta(bytes: Data) throws -> CoreDeltaApplied { try check(); return try inner.applyDelta(bytes: bytes) }
    func needsResync() throws -> Bool { try check(); return try inner.needsResync() }
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
