import Foundation
import Testing
import JarvisCore
import Synchronization
@testable import JarvisKit

@Suite("LockSignals decision rule")
struct LockSignalsTests {
    private let app = LockSignals.Front.app("dev.jarvis.app")

    @Test("commands are refused when either reading says locked")
    func commands() {
        #expect(LockSignals(notify: true, lockScreen: nil, front: app).forCommands == .locked)
        #expect(LockSignals(notify: false, lockScreen: true, front: .springBoard).forCommands == .locked)
        #expect(LockSignals(notify: true, lockScreen: false, front: .springBoard).forCommands == .locked)
        #expect(LockSignals(notify: false, lockScreen: false, front: .springBoard).forCommands == .unlocked)
        #expect(LockSignals(notify: nil, lockScreen: nil, front: app).forCommands == .unlocked,
                "an app in front counts as no lock screen")
        #expect(LockSignals(notify: nil, lockScreen: nil, front: .springBoard).forCommands == .unknown)
    }

    @Test("a stale notify state never holds a resume: app in front or home screen is unlocked")
    func resume() {
        #expect(LockSignals(notify: true, lockScreen: nil, front: app).forResume == .unlocked)
        #expect(LockSignals(notify: true, lockScreen: false, front: .springBoard).forResume == .unlocked)
        #expect(LockSignals(notify: false, lockScreen: true, front: .springBoard).forResume == .locked)
        #expect(LockSignals(notify: true, lockScreen: nil, front: .springBoard).forResume == .locked,
                "without a lock-screen reading the notify state decides")
        #expect(LockSignals(notify: false, lockScreen: nil, front: .springBoard).forResume == .unlocked)
        #expect(LockSignals(notify: nil, lockScreen: nil, front: .springBoard).forResume == .unknown)
    }

    @Test("the description carries both raw readings and both decisions")
    func description() {
        let text = LockSignals(notify: true, lockScreen: false, front: .springBoard).description
        #expect(text == "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=false, front=SpringBoard -> commands locked, resume unlocked")
        #expect(LockSignals(notify: nil, lockScreen: nil, front: app).description.contains("notify(com.apple.springboard.lockstate)=unavailable, lock screen=not checked, front=dev.jarvis.app"))
    }
}

/// A runner whose lock flips between "before" and "after" a command.
private final class FlippingRunner: PhoneRunner {
    let lockAfterCommand: Mutex<Bool>
    let locked = Mutex(false)
    let lockReads = Mutex(0)

    init(lockAfterCommand: Bool) { self.lockAfterCommand = Mutex(lockAfterCommand) }

    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        RunnerObservation(root: DemoPhone.tree(for: .home), appBundleId: DemoPhone.bundleId, appName: nil, screenSize: DemoPhone.screenSize)
    }

    func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        // Unlocked when the command starts (the pre-check passes); the lock comes as it runs.
        if lockAfterCommand.withLock({ $0 }) { locked.withLock { $0 = true } }
        return RunnerResult(ok: true, error: nil)
    }

    func lockState() async throws -> DeviceLockState {
        lockReads.withLock { $0 += 1 }
        return locked.withLock { $0 } ? .locked : .unlocked
    }
}

@Suite("Lock check after a command")
struct PostCommandLockTests {
    @Test("a command that ran while the phone locked is reported as locked, not ok")
    func lockedAfter() async throws {
        let runner = FlippingRunner(lockAfterCommand: true)
        let result = try await LockCheckingRunner(runner).perform(.tap(x: 10, y: 20))
        #expect(!result.ok)
        #expect(result.failure == .deviceLocked)
        #expect(result.error?.contains("may not have taken effect") == true)
    }

    @Test("unlocked after: the result is kept; waits are not checked")
    func unlockedAfter() async throws {
        let runner = FlippingRunner(lockAfterCommand: false)
        let checked = LockCheckingRunner(runner)
        #expect(try await checked.perform(.tap(x: 1, y: 1)).ok)
        #expect(runner.lockReads.withLock { $0 } == 1)
        runner.lockAfterCommand.withLock { $0 = true }
        #expect(try await checked.perform(.wait(ms: 1)).ok, "a wait must not be turned into a lock failure")
        #expect(runner.lockReads.withLock { $0 } == 1, "a wait read the lock state")
    }

    @Test("the rule itself: unknown changes nothing; a failure already marked locked is kept")
    func rule() {
        let ok = RunnerResult(ok: true, error: nil)
        #expect(ok.checkedAfterCommand(.tap(x: 0, y: 0), lockedAfter: .unknown) == ok)
        #expect(ok.checkedAfterCommand(.tap(x: 0, y: 0), lockedAfter: .unlocked) == ok)
        let refused = RunnerResult.deviceLocked("refused before")
        #expect(refused.checkedAfterCommand(.tap(x: 0, y: 0), lockedAfter: .locked) == refused)
    }
}
