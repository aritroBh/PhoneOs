import Foundation
import Testing
import JarvisCore
import Synchronization
@testable import JarvisKit

@Suite("LockJudge decision rule", .timeLimit(.minutes(1)))
struct LockSignalsTests {
    private let app = LockSignals.Front.app("dev.jarvis.app")
    private func s(_ n: Bool?, _ l: Bool?, _ f: LockSignals.Front) -> LockSignals {
        LockSignals(notify: n, lockScreen: l, front: f)
    }

    @Test("every row of the rule, judged at one instant")
    func rows() {
        func state(_ x: LockSignals) -> DeviceLockState { LockJudge().judge(x, at: .zero).state }
        #expect(state(s(true, true, .springBoard)) == .locked)
        #expect(state(s(false, true, .springBoard)) == .locked)
        #expect(state(s(nil, true, .springBoard)) == .locked)
        #expect(state(s(true, nil, app)) == .locked, "fresh notify, app in front: locked (the failing run's state)")
        #expect(state(s(true, false, .springBoard)) == .locked)
        #expect(state(s(true, nil, .springBoard)) == .locked)
        #expect(state(s(false, false, .springBoard)) == .unlocked)
        #expect(state(s(false, nil, app)) == .unlocked)
        #expect(state(s(false, nil, .springBoard)) == .unlocked)
        #expect(state(s(nil, false, .springBoard)) == .unlocked)
        #expect(state(s(nil, nil, app)) == .unknown)
        #expect(state(s(nil, nil, .springBoard)) == .unknown)
    }

    @Test("the failing run on c0cff3e: locked from 18.8 s to 24.3 s, unlocked after (was: commands locked, resume unlocked)")
    func failingRun() {
        let judge = LockJudge()
        for t in [18.8, 19.9, 20.8, 21.8, 23.0] {
            let v = judge.judge(s(true, nil, app), at: .seconds(t))
            #expect(v.state == .locked, "t+\(t)s")
        }
        #expect(judge.judge(s(false, nil, app), at: .seconds(24.3)).state == .unlocked)
        #expect(judge.judge(s(false, nil, app), at: .seconds(25.0)).state == .unlocked)
    }

    @Test("notify stuck true with an app in front: stale after 12 s, one note, unlocked until notify reads false")
    func staleAfterTwelveSeconds() {
        let judge = LockJudge()
        var notes: [String] = []
        var states: [DeviceLockState] = []
        for t in stride(from: 0.0, through: 20.0, by: 1.0) {
            let v = judge.judge(s(true, nil, app), at: .seconds(t))
            states.append(v.state)
            if let n = v.staleNote { notes.append(n) }
        }
        #expect(states[0...11].allSatisfy { $0 == .locked })
        #expect(states[12...].allSatisfy { $0 == .unlocked })
        #expect(notes == ["the lock notification has said locked for 12.0 s with no lock screen; treated as stale"])
        // Reads false: the stretch resets; a new true reading starts a new 12 s.
        #expect(judge.judge(s(false, nil, app), at: .seconds(21)).state == .unlocked)
        #expect(judge.judge(s(true, nil, app), at: .seconds(22)).state == .locked)
        #expect(judge.judge(s(true, nil, app), at: .seconds(33.9)).state == .locked)
        #expect(judge.judge(s(true, nil, app), at: .seconds(34)).staleNote != nil)
    }

    @Test("the stale time is configurable")
    func configurable() {
        let judge = LockJudge(notifyStaleAfter: .seconds(2))
        #expect(judge.judge(s(true, false, .springBoard), at: .zero).state == .locked)
        #expect(judge.judge(s(true, false, .springBoard), at: .seconds(2)).state == .unlocked)
    }

    @Test("a confirmed lock screen in the stretch restarts the staleness clock and clears a stale verdict")
    func confirmedResets() {
        let judge = LockJudge()
        #expect(judge.judge(s(true, nil, app), at: .zero).state == .locked)
        #expect(judge.judge(s(true, true, .springBoard), at: .seconds(10)).state == .locked)
        #expect(judge.judge(s(true, false, .springBoard), at: .seconds(21)).state == .locked, "11 s since the confirmation")
        #expect(judge.judge(s(true, false, .springBoard), at: .seconds(22)).state == .unlocked, "12 s since it")
        let again = judge.judge(s(true, true, .springBoard), at: .seconds(23))
        #expect(again.state == .locked && again.staleNote == nil, "stale is cleared by a real lock screen")
        #expect(judge.judge(s(true, nil, app), at: .seconds(30)).state == .locked)
    }

    @Test("lock screen confirmed wins over notify false; notify nil falls back to the lock screen")
    func others() {
        let judge = LockJudge()
        #expect(judge.judge(s(false, true, .springBoard), at: .zero).state == .locked)
        #expect(judge.judge(s(nil, true, .springBoard), at: .seconds(1)).state == .locked)
        #expect(judge.judge(s(nil, false, .springBoard), at: .seconds(2)).state == .unlocked)
        #expect(judge.judge(s(nil, nil, .springBoard), at: .seconds(3)).state == .unknown)
    }

    @Test("the verdict text: raw readings plus the one verdict and how long notify has been true")
    func description() {
        let judge = LockJudge()
        _ = judge.judge(s(true, false, .springBoard), at: .seconds(1))
        let v = judge.judge(s(true, false, .springBoard), at: .seconds(3.5))
        let text = "\(s(true, false, .springBoard)) \(v)"
        #expect(text == "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=false, front=SpringBoard -> locked, notify true for 2.5 s")
        #expect(s(nil, nil, app).description.contains("notify(com.apple.springboard.lockstate)=unavailable, lock screen=not checked, front=dev.jarvis.app"))
    }

    @Test("one verdict: over random reading sequences a command is never refused while resume says unlocked")
    func neverContradicts() {
        var rng = SystemRandomNumberGenerator()
        let fronts: [LockSignals.Front] = [app, .springBoard]
        let tri: [Bool?] = [true, false, nil]
        for _ in 0..<200 {
            let judge = LockJudge(notifyStaleAfter: .seconds(Double.random(in: 1...6, using: &rng)))
            var t = 0.0
            for _ in 0..<60 {
                t += Double.random(in: 0...3, using: &rng)
                let signals = s(tri.randomElement(using: &rng)!, tri.randomElement(using: &rng)!, fronts.randomElement(using: &rng)!)
                let verdict = judge.judge(signals, at: .seconds(t))
                // What the runners do: the same verdict answers both questions.
                let commandRefused = verdict.state == .locked
                let resumeState = verdict.state
                #expect(!(commandRefused && resumeState == .unlocked))
                if signals.lockScreen == true { #expect(verdict.state == .locked) }
            }
        }
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

@Suite("Lock check after a command", .timeLimit(.minutes(1)))
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
        #expect(runner.lockReads.withLock { $0 } == 2, "one read before the command, one after")
        runner.lockAfterCommand.withLock { $0 = true }
        #expect(try await checked.perform(.wait(ms: 1)).ok, "a wait must not be turned into a lock failure")
        #expect(runner.lockReads.withLock { $0 } == 2, "a wait read the lock state")
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
