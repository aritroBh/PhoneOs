import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

// Review probes (approval preview). Each test asserts the SAFE behaviour.

private let livePixels = CapturedScreenshot(data: Data(repeating: 0x5A, count: 32), mimeType: "image/png",
                                            pixelWidth: 393, pixelHeight: 852)

/// The demo phone whose screen moves on right after it was read: a capture taken after a
/// tree read shows the next screen, and the next tree read shows it too.
private final class MovingPhone: PhoneRunner, Sendable {
    private struct State {
        var captures = 0
        var moved = false
    }

    let inner = DemoPhone()
    private let state = Mutex(State())

    var captures: Int { state.withLock { $0.captures } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let moved = state.withLock { s -> Bool in
            defer { s.moved = false }
            return s.moved
        }
        var o = try await inner.snapshot(includeScreenshot: includeScreenshot)
        if moved, var root = o.root {
            root.children.append(UiNode(role: .staticText, label: "Moved on", value: nil, identifier: nil,
                                        placeholder: nil, frame: Rect(x: 16, y: 800, width: 200, height: 20), enabled: true,
                                        visible: true, hittable: false, focused: false, selected: false, secure: false,
                                        children: []))
            o.ui = .tree(root)
        }
        return o
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult { try await inner.perform(command) }

    func lockState() async throws -> DeviceLockState { .unlocked }

    func viewerScreenshot() async throws -> CapturedScreenshot? {
        // The pixels are of a screen that is no longer the one just read.
        state.withLock { $0.captures += 1; $0.moved = true }
        return livePixels
    }
}

private final class RecordingGate: HumanGate {
    private let store: ApprovalFrameStore
    private let log = Mutex<[(key: ViewerFrameKey?, frame: CapturedScreenshot?)]>([])

    init(store: ApprovalFrameStore) { self.store = store }

    var asked: [(key: ViewerFrameKey?, frame: CapturedScreenshot?)] { log.withLock { $0 } }

    func confirm(_ request: ConfirmRequest) async throws -> Bool {
        let key = store.key
        let frame = key.flatMap { store.frame(for: $0) }
        log.withLock { $0.append((key, frame)) }
        return false
    }

    func answer(_ question: String) async throws -> String { "" }
}

private func drive(_ phone: any PhoneRunner, store: ApprovalFrameStore, gate: RecordingGate,
                   viewerFrames: ViewerFrameStore?) async throws -> RunResult {
    let model = ScriptedModel(steps: DemoPhone.deleteAccountScript,
                              capabilities: ModelCapabilities(contextTokens: 8_192, supportsVision: false))
    let config = try SessionConfigBuilder.make(capabilities: model.capabilities, knownApps: DemoPhone.knownApps,
                                               adjustPolicy: { $0.firstUseApproval = false; $0.maxSteps = 40 })
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: gate, memory: RecordingMemory(),
                                  screenshots: ScreenshotStore(), clock: SteppingClock(), viewerFrames: viewerFrames,
                                  approvalFrames: store)
    let driver = try SessionDriver(
        config: config, task: TaskSpec(id: "probe", text: "Delete my account"), memoryJSON: "", dependencies: deps,
        configuration: DriverConfiguration(observeRetryDelay: .milliseconds(10), humanTimeout: .seconds(600),
                                           unlockPollInterval: .milliseconds(5), lockWatchInterval: nil,
                                           viewerScreenshots: viewerFrames != nil, viewerScreenshotTimeout: .seconds(1),
                                           approvalScreenshots: true)
    )
    let drain = Task { for await _ in driver.updates {} }
    let result = await driver.run()
    _ = await drain.value
    return result
}

@Suite("Review probes: approval preview", .timeLimit(.minutes(2)), .serialized)
struct ReviewProbePreviewTests {
    /// Baseline (holds): the fresh capture of a screen that moved on is refused.
    @Test("baseline: the fresh capture of a screen that moved on keeps nothing")
    func freshCaptureOfAMovedScreenIsRefused() async throws {
        let store = ApprovalFrameStore()
        let phone = MovingPhone()
        let gate = RecordingGate(store: store)
        _ = try await drive(phone, store: store, gate: gate, viewerFrames: nil)
        #expect(phone.captures == 1)
        #expect(gate.asked.first?.frame == nil, "the fresh path re-reads the tree and drops the frame")
    }

    /// P2: with the Live view option on, the Live view's frame of the evaluated observation
    /// is reused as "What Jarvis sees now" without the tree re-read the fresh path requires:
    /// pixels of a screen that had already moved on are shown as the evaluated screen.
    @Test("the Live view's frame of a screen that moved on is not shown as the evaluated screen")
    func liveViewFrameOfAMovedScreenIsNotReused() async throws {
        let store = ApprovalFrameStore()
        let phone = MovingPhone()
        let gate = RecordingGate(store: store)
        _ = try await drive(phone, store: store, gate: gate, viewerFrames: ViewerFrameStore())
        let first = try #require(gate.asked.first)
        #expect(first.frame == nil,
                "pixels taken after the evaluated tree, of a screen that moved on, shown as that screen: key \(String(describing: first.key))")
    }

    /// P1: the real frame is drawn aspect-fit into a box of the SCREEN's aspect and the outline
    /// is placed in that box's unit coordinates; nothing checks that the frame has the
    /// screen's aspect (e.g. a landscape capture of a portrait tree), so the outline lands on
    /// another part of the picture. Safe: such a frame is not shown as the real screen.
    @Test("a frame whose aspect differs from the evaluated screen's is not drawn under the outline")
    func mismatchedFrameIsNotShown() throws {
        let screen = DemoPhone.screenSize
        let target = CoreApprovalTarget(
            frameKey: CoreViewerFrameKey(taskId: "t", observation: 3), screenId: 1, screenSize: screen,
            request: ConfirmRequest(category: .payment, summary: "Pay", app: "com.example.shop"),
            mark: .outline(frame: Rect(x: 16, y: 700, width: 361, height: 50),
                           unit: Rect(x: 16 / screen.width, y: 700 / screen.height, width: 361 / screen.width,
                                      height: 50 / screen.height)),
            caption: "Jarvis will tap the outlined control.", accessibilityLabel: "Outlined: Button 'Pay'"
        )
        let store = ApprovalFrameStore()
        let sideways = CapturedScreenshot(data: Data(repeating: 1, count: 16), mimeType: "image/png",
                                          pixelWidth: UInt32(screen.height * 3), pixelHeight: UInt32(screen.width * 3))
        store.put(sideways, for: ViewerFrameKey(taskId: "t", observation: 3))
        let preview = try #require(ApprovalPreview.make(prompt: .confirm(id: 1, request: target.request), target: target,
                                                        frames: store, appActive: true))
        #expect(preview.backdrop == .layout,
                "a \(sideways.pixelWidth)x\(sideways.pixelHeight) frame drawn as the \(screen.width)x\(screen.height) screen")
    }
}
