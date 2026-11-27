import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// The driver's real-screen option (pixels stay in Swift). The reducer, typing, encoding and
// scene tests moved with that logic to core/jarvis-viewer/tests/timeline.rs; the demo run
// through the coordinator is in LiveViewModelTests.

// MARK: - The driver's frame option

/// A phone that counts screenshot requests and returns pixels when asked (in a snapshot or
/// through the Live view's own capture).
private final class CountingPhone: PhoneRunner {
    let inner: any PhoneRunner
    private let requests = Mutex<[Bool]>([])
    private let viewerCalls = Mutex(0)

    init(_ inner: any PhoneRunner) { self.inner = inner }

    var screenshotRequests: [Bool] { requests.withLock { $0 } }
    var viewerScreenshotCalls: Int { viewerCalls.withLock { $0 } }

    func viewerScreenshot() async throws -> CapturedScreenshot? {
        viewerCalls.withLock { $0 += 1 }
        return CapturedScreenshot(data: Data(repeating: 2, count: 64), mimeType: "image/jpeg", pixelWidth: 393, pixelHeight: 852)
    }

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

    // Before the review every observation asked the runner for pixels when the option was
    // on; now the core's observation is taken exactly as with the option off and the
    // viewer's pixels come from a separate capture (or reuse the core's own).
    @Test("on: the core's observations are as with the option off; the viewer gets a frame of each, the core an ImageRef only when it asked")
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
            #expect(phone.screenshotRequests == seen.map(\.asked), "the runner is asked for pixels exactly when the core asks")
            #expect(phone.viewerScreenshotCalls == seen.filter { !$0.asked }.count,
                    "one separate capture per observation the core asked no pixels for; the core's own pixels are reused")
            for o in seen {
                #expect(o.gotRef == o.asked, "observation #\(o.number): the core asked \(o.asked)")
            }
            #expect(screenshots.storedIDs.count == seen.filter(\.asked).count, "the model's store only holds what the core asked for")
            #expect(Set(frames.keys) == Set(seen.map { ViewerFrameKey(taskId: "frames", observation: $0.number) }))
            // The reducer points each screen at its frame.
            let viewer = LiveViewModel(framesEnabled: true)
            for u in run.updates { viewer.apply(.update(u)) }
            let timeline = viewer.timeline
            #expect(!timeline.steps.isEmpty)
            for step in timeline.steps {
                let key = try #require(viewer.frameKey(for: timeline.screen(id: step.screenId)))
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
