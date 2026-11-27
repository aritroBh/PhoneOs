import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

/// A runner whose lock answers contradict (the XCUITest runner before `LockJudge`): while
/// `reallyLocked` it refuses every command as locked, yet `lockState()` and snapshots say
/// unlocked (an app in front). Records when each command was sent.
final class ContradictoryPhone: PhoneRunner {
    let inner = FakePhone()
    let reallyLocked = Mutex(false)
    let origin = ContinuousClock.now
    /// Times (since creation) of commands that were refused.
    let refusedAt = Mutex<[Duration]>([])
    let lockOnFirstCommand: Bool

    init(lockOnFirstCommand: Bool) { self.lockOnFirstCommand = lockOnFirstCommand }

    func unlock() { reallyLocked.withLock { $0 = false } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        try await inner.snapshot(includeScreenshot: includeScreenshot)
    }

    func lockState() async throws -> DeviceLockState { .unlocked }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        let refuse = reallyLocked.withLock { locked -> Bool in
            if lockOnFirstCommand, refusedAt.withLock({ $0.isEmpty }), !locked { locked = true }
            return locked
        }
        if refuse {
            refusedAt.withLock { $0.append(ContinuousClock.now - origin) }
            return .deviceLocked()
        }
        return try await inner.perform(command)
    }
}

@Suite("SessionDriver with a runner whose lock answers contradict", .timeLimit(.minutes(1)))
struct DriverContradictoryLockTests {
    private func run(_ phone: ContradictoryPhone, whileRunning: (@Sendable () async -> Void)? = nil) async throws -> (RunResult, [DriverUpdate]) {
        let model = FakeModel()
        let config = try SessionConfigBuilder.make(
            capabilities: model.capabilities,
            knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")],
            adjustPolicy: { $0.firstUseApproval = false }
        )
        let deps = DriverDependencies(
            runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(), memory: RecordingMemory(),
            screenshots: ScreenshotStore(), attention: RecordingAttention(), clock: SteppingClock()
        )
        // Default lockWatchInterval (3 s): the strict wait's gap.
        let configuration = DriverConfiguration(observeRetryDelay: .milliseconds(1), unlockPollInterval: .milliseconds(1))
        let driver = try SessionDriver(
            config: config, task: TaskSpec(id: "contradict", text: "Turn off Wi-Fi"), memoryJSON: "",
            dependencies: deps, configuration: configuration
        )
        let collector = Task { () -> [DriverUpdate] in
            var all: [DriverUpdate] = []
            for await u in driver.updates { all.append(u) }
            return all
        }
        if let whileRunning { Task { await whileRunning() } }
        let result = await driver.run()
        return (result, await collector.value)
    }

    @Test("a command refused as locked while lockState() says unlocked cannot make the driver spin; the run continues after the real unlock")
    func contradictoryRunnerDoesNotSpin() async throws {
        let phone = ContradictoryPhone(lockOnFirstCommand: true)
        let (result, updates) = try await run(phone, whileRunning: {
            try? await Task.sleep(for: .milliseconds(2_200))
            phone.unlock()
        })

        let refused = phone.refusedAt.withLock { $0 }
        #expect(refused.count >= 2, "the guard needs a second refusal to engage: \(refused)")
        #expect(refused.filter { $0 < .seconds(2) }.count <= 2, "refusals in the first 2 s: \(refused)")
        let ended = updates.filter { $0 == .unlockWaitEnded }.count
        // One short wait ended before the guard could know (the first refusal), one after the real unlock.
        #expect(ended <= 2, "unlock waits ended: \(ended)")
        #expect(result.outcome.status == .success, "\(result.outcome.status): \(result.outcome.summary)")
        #expect(phone.inner.page == .wifiOff)
    }

    @Test("a normal lock and unlock still resumes promptly")
    func normalUnlockIsPrompt() async throws {
        let phone = LockablePhone(locked: true, unlockAfterPolls: 4)
        let started = ContinuousClock.now
        let model = FakeModel()
        let config = try SessionConfigBuilder.make(
            capabilities: model.capabilities,
            knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")],
            adjustPolicy: { $0.firstUseApproval = false }
        )
        let deps = DriverDependencies(
            runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(), memory: RecordingMemory(),
            screenshots: ScreenshotStore(), attention: RecordingAttention(), clock: SteppingClock()
        )
        let driver = try SessionDriver(
            config: config, task: TaskSpec(id: "normal", text: "Turn off Wi-Fi"), memoryJSON: "", dependencies: deps,
            configuration: DriverConfiguration(observeRetryDelay: .milliseconds(1), unlockPollInterval: .milliseconds(1))
        )
        let result = await driver.run()
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(ContinuousClock.now - started < .seconds(2), "no strict wait for a first, normal lock")
    }
}
