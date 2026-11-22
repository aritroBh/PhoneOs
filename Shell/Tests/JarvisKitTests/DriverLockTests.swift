import Foundation
import JarvisCore
import Synchronization
import Testing
@testable import JarvisKit

/// The fake Settings phone (`FakePhone`) with a lock: while locked, snapshots are of the
/// lock screen (`deviceLocked`), every command is refused with `RunnerFailure.deviceLocked`
/// and nothing changes; `lockState()` reports the truth. The lock can be scheduled.
final class LockablePhone: PhoneRunner {
    struct State {
        var locked = false
        /// Lock-state polls (and locked snapshots) left before the phone unlocks itself
        /// (`nil`: stays as it is).
        var unlockAfterPolls: Int?
        /// Lock just before this (1-based) command is performed.
        var lockBeforeCommand: Int?
        var commandsSeen = 0
        var polls = 0
        var refused: [RunnerCommand] = []
        /// Commands run since the last unlock without a snapshot in between.
        var staleActs = 0
        var unlockedSinceSnapshot = false
    }

    let inner = FakePhone()
    let state = Mutex(State())

    init(locked: Bool = false, unlockAfterPolls: Int? = nil, lockBeforeCommand: Int? = nil) {
        state.withLock {
            $0.locked = locked
            $0.unlockAfterPolls = unlockAfterPolls
            $0.lockBeforeCommand = lockBeforeCommand
        }
    }

    func lock(unlockAfterPolls: Int?) {
        state.withLock {
            $0.locked = true
            $0.unlockAfterPolls = unlockAfterPolls
        }
    }

    var snapshotState: State { state.withLock { $0 } }

    /// One poll: counts towards the scheduled unlock.
    private func poll() -> Bool {
        state.withLock { s -> Bool in
            s.polls += 1
            if s.locked, let left = s.unlockAfterPolls {
                if left <= 1 {
                    s.locked = false
                    s.unlockAfterPolls = nil
                    s.unlockedSinceSnapshot = true
                } else {
                    s.unlockAfterPolls = left - 1
                }
            }
            return s.locked
        }
    }

    func lockState() async throws -> DeviceLockState {
        poll() ? .locked : .unlocked
    }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let locked = state.withLock { s -> Bool in
            s.unlockedSinceSnapshot = false
            return s.locked
        }
        if locked {
            return RunnerObservation(
                source: "Application, label: 'SpringBoard', {{0, 0}, {393, 852}}",
                appBundleId: "com.apple.springboard", screenSize: screenSize, deviceLocked: true
            )
        }
        return try await inner.snapshot(includeScreenshot: includeScreenshot)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        let refuse = state.withLock { s -> Bool in
            s.commandsSeen += 1
            if s.lockBeforeCommand == s.commandsSeen { s.locked = true }
            if s.locked {
                s.refused.append(command)
                return true
            }
            if s.unlockedSinceSnapshot { s.staleActs += 1 }
            return false
        }
        if refuse { return .deviceLocked() }
        return try await inner.perform(command)
    }
}

/// Records `OwnerAttention` calls.
final class RecordingAttention: OwnerAttention {
    let calls = Mutex<[String]>([])

    func awaitUnlock(reason: String, waitedMs: UInt64) async {
        calls.withLock { $0.append("await") }
    }

    func unlockWaitEnded() async {
        calls.withLock { $0.append("ended") }
    }

    var log: [String] { calls.withLock { $0 } }
}

/// Runs the Wi-Fi task on `phone`.
private func runLocked(
    _ phone: any PhoneRunner,
    gate: FakeGate = FakeGate(),
    attention: RecordingAttention = RecordingAttention(),
    maxLockedWaitMs: UInt64? = nil,
    lockWatch: Duration? = .milliseconds(5),
    whileRunning: (@Sendable (SessionDriver) async -> Void)? = nil
) async throws -> (RunResult, [DriverUpdate]) {
    let model = FakeModel()
    let config = try SessionConfigBuilder.make(
        capabilities: model.capabilities,
        knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")],
        adjustPolicy: { policy in
            policy.firstUseApproval = false
            if let maxLockedWaitMs { policy.maxLockedWaitMs = maxLockedWaitMs }
        }
    )
    let deps = DriverDependencies(
        runner: phone, model: model, tools: ToolRegistry(), human: gate, memory: RecordingMemory(),
        screenshots: ScreenshotStore(), attention: attention, clock: SteppingClock()
    )
    let configuration = DriverConfiguration(
        observeRetryDelay: .milliseconds(1), unlockPollInterval: .milliseconds(1), lockWatchInterval: lockWatch
    )
    let driver = try SessionDriver(
        config: config, task: TaskSpec(id: "lock", text: "Turn off Wi-Fi"), memoryJSON: "",
        dependencies: deps, configuration: configuration
    )
    let collector = Task { () -> [DriverUpdate] in
        var all: [DriverUpdate] = []
        for await u in driver.updates { all.append(u) }
        return all
    }
    if let whileRunning { Task { await whileRunning(driver) } }
    let result = await driver.run()
    return (result, await collector.value)
}

private extension [DriverUpdate] {
    var awaitingUnlock: [UInt64] {
        compactMap { if case .awaitingUnlock(_, let waited) = $0 { return waited } else { return nil } }
    }

    var events: [Event] {
        compactMap { if case .effectCompleted(_, let e) = $0 { return e } else { return nil } }
    }

    var effects: [Effect] {
        compactMap { if case .effect(let e, _) = $0 { return e } else { return nil } }
    }

    func count(_ match: (DriverUpdate) -> Bool) -> Int { filter(match).count }
}

@Suite("SessionDriver with a locked phone (real core)", .timeLimit(.minutes(1)))
struct DriverLockTests {
    @Test("locked before the first observation: wait, tick, unlock, then the task finishes")
    func lockedAtStart() async throws {
        let phone = LockablePhone(locked: true, unlockAfterPolls: 4)
        let attention = RecordingAttention()
        let (result, updates) = try await runLocked(phone, attention: attention)

        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(!updates.awaitingUnlock.isEmpty, "the driver reported the wait")
        #expect(updates.events.contains(.tick))
        #expect(updates.events.contains(.deviceUnlocked))
        #expect(attention.log == ["await", "ended"], "the owner is told once per wait")
        #expect(updates.count { $0 == .unlockWaitEnded } == 1)
        #expect(phone.snapshotState.staleActs == 0)
        #expect(phone.inner.page == .wifiOff)
    }

    @Test("a command refused because the phone locked: nothing ran, re-observed after the unlock")
    func refusedMidAct() async throws {
        let phone = LockablePhone(lockBeforeCommand: 2)
        phone.state.withLock { $0.unlockAfterPolls = 3 }
        let (result, updates) = try await runLocked(phone)

        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(phone.snapshotState.refused.count == 1)
        #expect(updates.events.contains(.actResult(result: .deviceLocked())))
        #expect(!updates.awaitingUnlock.isEmpty)
        #expect(phone.snapshotState.staleActs == 0, "no command between the unlock and the next observation")
        // After deviceUnlocked the next effect is an observation, never an act.
        let effects = updates.effects
        if let unlock = updates.firstIndex(where: {
            if case .effectCompleted(_, .deviceUnlocked) = $0 { return true } else { return false }
        }) {
            let next = updates[(unlock + 1)...].first { if case .effect = $0 { return true } else { return false } }
            if case .effect(let effect, _)? = next {
                #expect({ if case .observe = effect { return true } else { return false } }(), "after unlock: \(effect)")
            } else {
                Issue.record("no effect after the unlock")
            }
        } else {
            Issue.record("no deviceUnlocked event; effects: \(effects)")
        }
        #expect(phone.inner.page == .wifiOff)
    }

    @Test("a lock while a confirmation is pending: the wait is abandoned and the approval asked again")
    func lockDuringConfirmation() async throws {
        let phone = LockablePhone()
        let latch = Latch()
        // The first confirmation never answers (it ignores cancellation); meanwhile the
        // phone locks. After the unlock the second confirmation is approved.
        let gate = LockingGate(phone: phone, latch: latch)
        let (result, updates) = try await runLocked(phone, gate: gate.inner)

        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(updates.contains(.lockDetected(.confirm)))
        #expect(gate.inner.confirms.count == 2, "asked again after the unlock: \(gate.inner.confirms)")
        #expect(gate.inner.confirms.allSatisfy { $0.category == .systemSetting })
        #expect(phone.inner.page == .wifiOff)
        #expect(phone.snapshotState.staleActs == 0)
        latch.release()
    }

    @Test("stays locked past maxLockedWaitMs: Interrupted, not Failed")
    func interrupted() async throws {
        let phone = LockablePhone(locked: true)
        let attention = RecordingAttention()
        let (result, updates) = try await runLocked(phone, attention: attention, maxLockedWaitMs: 2_000)

        #expect(result.outcome.status == .interrupted, "\(result.outcome.status): \(result.outcome.summary)")
        #expect(result.endReason == .sessionFinished)
        #expect(updates.awaitingUnlock.count > 3)
        #expect(updates.awaitingUnlock == updates.awaitingUnlock.sorted(), "waited time only grows")
        #expect(attention.log == ["await", "ended"])
        #expect(phone.snapshotState.refused.isEmpty)
    }

    @Test("cancel while waiting for an unlock stops the run at once")
    func cancelWhileLocked() async throws {
        let phone = LockablePhone(locked: true)
        let attention = RecordingAttention()
        let (result, updates) = try await runLocked(phone, attention: attention, whileRunning: { driver in
            try? await waitUntil { attention.log.contains("await") }
            await driver.cancel()
        })
        #expect(result.endReason == .cancelled)
        #expect(result.outcome.status == .cancelled)
        #expect(!updates.awaitingUnlock.isEmpty)
        #expect(attention.log == ["await", "ended"], "a cancelled run still clears the notice")
    }

    @Test("a runner that cannot tell its lock state answers the wait with observations")
    func unknownLockState() async throws {
        let phone = UnknownLockPhone(lockedSnapshots: 3)
        let (result, updates) = try await runLockedUnknown(phone)
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(!updates.awaitingUnlock.isEmpty)
        #expect(!updates.events.contains(.tick), "no tick without a lock state")
    }
}

/// A gate whose first confirmation locks the phone and then hangs; later ones approve.
final class LockingGate: Sendable {
    let inner: FakeGate

    init(phone: LockablePhone, latch: Latch) {
        let asked = Mutex(0)
        inner = FakeGate(confirmReply: .value)
        inner.beforeConfirm = { @Sendable in
            let n = asked.withLock { n -> Int in
                n += 1
                return n
            }
            if n == 1 {
                phone.lock(unlockAfterPolls: 3)
                await latch.hang()
            }
        }
    }
}

/// Locked for the first `lockedSnapshots` snapshots, with no lock-state API.
final class UnknownLockPhone: PhoneRunner {
    let inner = FakePhone()
    let left: Mutex<Int>

    init(lockedSnapshots: Int) { left = Mutex(lockedSnapshots) }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let locked = left.withLock { n -> Bool in
            defer { n = max(0, n - 1) }
            return n > 0
        }
        if locked {
            return RunnerObservation(source: "Application, {{0, 0}, {393, 852}}", appBundleId: "com.apple.springboard",
                                     screenSize: screenSize, deviceLocked: true)
        }
        return try await inner.snapshot(includeScreenshot: includeScreenshot)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        try await inner.perform(command)
    }
}

private func runLockedUnknown(_ phone: UnknownLockPhone) async throws -> (RunResult, [DriverUpdate]) {
    let model = FakeModel()
    let config = try SessionConfigBuilder.make(
        capabilities: model.capabilities,
        knownApps: [AppInfo(bundleId: settingsBundle, name: "Settings")],
        adjustPolicy: { $0.firstUseApproval = false }
    )
    let deps = DriverDependencies(runner: phone, model: model, tools: ToolRegistry(), human: FakeGate(),
                                  memory: RecordingMemory(), screenshots: ScreenshotStore(), clock: SteppingClock())
    let driver = try SessionDriver(
        config: config, task: TaskSpec(id: "u", text: "Turn off Wi-Fi"), memoryJSON: "", dependencies: deps,
        configuration: DriverConfiguration(observeRetryDelay: .milliseconds(1), unlockPollInterval: .milliseconds(1))
    )
    let collector = Task { () -> [DriverUpdate] in
        var all: [DriverUpdate] = []
        for await u in driver.updates { all.append(u) }
        return all
    }
    let result = await driver.run()
    return (result, await collector.value)
}

/// A phone like a LoopCtrl runner: it does NOT refuse commands while locked (a tap lands on
/// the lock screen and does nothing), and reports its lock state only when `canTell`. It
/// locks right after snapshot number `lockAfterSnapshot` returns (so between the
/// observation and the command the model chose from it) and unlocks after
/// `unlockAfterPolls` lock-state polls.
final class TapsThroughLockPhone: PhoneRunner {
    struct State {
        var locked = false
        var snapshots = 0
        var lockAfterSnapshot: Int?
        var unlockAfterPolls = 3
        /// Commands that reached the phone while it was locked (they did nothing).
        var sentWhileLocked: [RunnerCommand] = []
        var sent: [RunnerCommand] = []
    }

    let inner = FakePhone()
    let state = Mutex(State())
    let canTell: Bool

    init(lockAfterSnapshot: Int?, canTell: Bool = true) {
        self.canTell = canTell
        state.withLock { $0.lockAfterSnapshot = lockAfterSnapshot }
    }

    var snapshotState: State { state.withLock { $0 } }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        let locked = state.withLock { $0.locked }
        if locked {
            return RunnerObservation(source: "Application, label: 'SpringBoard', {{0, 0}, {393, 852}}",
                                     appBundleId: "com.apple.springboard", screenSize: screenSize, deviceLocked: true)
        }
        let observation = try await inner.snapshot(includeScreenshot: includeScreenshot)
        state.withLock { s in
            s.snapshots += 1
            if s.snapshots == s.lockAfterSnapshot { s.locked = true }
        }
        return observation
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        let locked = state.withLock { s -> Bool in
            s.sent.append(command)
            if s.locked { s.sentWhileLocked.append(command) }
            return s.locked
        }
        if locked { return RunnerResult(ok: true, error: nil) }
        return try await inner.perform(command)
    }

    func lockState() async throws -> DeviceLockState {
        guard canTell else { return .unknown }
        return state.withLock { s -> DeviceLockState in
            if s.locked {
                s.unlockAfterPolls -= 1
                if s.unlockAfterPolls <= 0 { s.locked = false }
            }
            return s.locked ? .locked : .unlocked
        }
    }
}

@Suite("Lock check before a command (LockCheckingRunner, real core)", .timeLimit(.minutes(1)))
struct PreCommandLockTests {
    @Test("the phone locks between the observation and the command: nothing is sent, the run waits, re-observes and chooses again")
    func lockedBeforeCommand() async throws {
        let phone = TapsThroughLockPhone(lockAfterSnapshot: 1)
        // No lock watch: the lock lands after the model answered, right before the command.
        let (result, updates) = try await runLocked(LockCheckingRunner(phone), lockWatch: nil)

        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(phone.snapshotState.sentWhileLocked.isEmpty, "a command was sent to a locked phone")
        let lockedActs = updates.events.filter {
            if case .actResult(let r) = $0 { return r.failure == .deviceLocked && !r.ok } else { return false }
        }
        #expect(lockedActs.count == 1, "the refused command was not reported as locked: \(updates.events)")
        #expect(!updates.awaitingUnlock.isEmpty, "the run did not wait for the unlock")
        #expect(updates.events.contains(.deviceUnlocked))
        // After the unlock the next effect is an observation, then the model chooses again.
        guard let unlock = updates.firstIndex(where: {
            if case .effectCompleted(_, .deviceUnlocked) = $0 { return true } else { return false }
        }) else {
            Issue.record("no deviceUnlocked")
            return
        }
        let after = updates[(unlock + 1)...].compactMap { u -> Effect? in if case .effect(let e, _) = u { return e } else { return nil } }
        #expect({ if case .observe? = after.first { return true } else { return false } }(), "after unlock: \(after.prefix(2))")
        #expect(after.contains { if case .infer = $0 { return true } else { return false } }, "the action was not chosen again")
        #expect(phone.inner.page == .wifiOff)
    }

    @Test("a runner that cannot tell its lock state: commands are sent as before")
    func unknownLockStateStillWorks() async throws {
        let phone = TapsThroughLockPhone(lockAfterSnapshot: nil, canTell: false)
        let (result, _) = try await runLocked(LockCheckingRunner(phone), lockWatch: nil)
        #expect(result.outcome.status == .success, "\(result.outcome.summary)")
        #expect(!phone.snapshotState.sent.isEmpty)
        #expect(phone.inner.page == .wifiOff)
    }

    @Test("a locked reading before the command: not sent, the runner's own locked refusal; a wait is not checked")
    func refusalShape() async throws {
        let phone = TapsThroughLockPhone(lockAfterSnapshot: 1)
        let checked = LockCheckingRunner(phone)
        _ = try await checked.snapshot(includeScreenshot: false)
        phone.state.withLock { $0.unlockAfterPolls = 100 }
        let result = try await checked.perform(.tap(x: 1, y: 1))
        #expect(!result.ok)
        #expect(result.failure == .deviceLocked)
        #expect(phone.snapshotState.sent.isEmpty, "the tap reached the phone")
        #expect(try await checked.perform(.wait(ms: 1)).ok)
        #expect(phone.snapshotState.sent.count == 1, "the wait was not passed through")
    }
}
