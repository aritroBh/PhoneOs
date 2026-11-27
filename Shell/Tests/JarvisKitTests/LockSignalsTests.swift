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

    @Test("notify stuck true with an app in front, SpringBoard checked and showing no lock screen: stale after 12 s, one note, unlocked until notify reads false")
    func staleAfterTwelveSeconds() {
        let judge = LockJudge()
        var notes: [String] = []
        var states: [DeviceLockState] = []
        for t in stride(from: 0.0, through: 20.0, by: 1.0) {
            let v = judge.judge(s(true, false, app), at: .seconds(t))
            states.append(v.state)
            if let n = v.staleNote { notes.append(n) }
        }
        #expect(states[0...11].allSatisfy { $0 == .locked })
        #expect(states[12...].allSatisfy { $0 == .unlocked })
        #expect(notes == ["the lock notification has said locked for 12.0 s with no lock screen; treated as stale"])
        // Reads false: the stretch resets; a new true reading starts a new 12 s.
        #expect(judge.judge(s(false, nil, app), at: .seconds(21)).state == .unlocked)
        #expect(judge.judge(s(true, false, app), at: .seconds(22)).state == .locked)
        #expect(judge.judge(s(true, false, app), at: .seconds(33.9)).state == .locked)
        #expect(judge.judge(s(true, false, app), at: .seconds(34)).staleNote != nil)
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

    @Test("audit C6: notify true and the lock screen not checked is locked however long it lasts (was: unlocked, stale, after 12 s)")
    func uncheckedNeverStale() {
        let judge = LockJudge()
        for t in stride(from: 0.0, through: 60.0, by: 0.5) {
            let v = judge.judge(s(true, nil, app), at: .seconds(t))
            #expect(v.state == .locked, "t+\(t)s")
            #expect(v.staleNote == nil, "t+\(t)s")
        }
        // Stale on checked readings, then an unchecked one: locked again (the check could not
        // be made, so nothing positively shows the phone unlocked).
        let other = LockJudge()
        _ = other.judge(s(true, false, app), at: .zero)
        #expect(other.judge(s(true, false, app), at: .seconds(12)).state == .unlocked)
        #expect(other.judge(s(true, nil, app), at: .seconds(13)).state == .locked)
        #expect(other.judge(s(true, false, app), at: .seconds(14)).state == .unlocked, "no second stale note needed")
        #expect(other.judge(s(true, false, app), at: .seconds(14)).staleNote == nil)
    }

    /// The real lock signal lines of run 137 (`LockScreenProbeTests`, step 3a right after the
    /// lock button and its unlock-wait polls, then step 3d about 8.7 s later; the lines as
    /// the runner wrote them in the observation notes), parsed back into readings.
    static let run137LockProbe: [(seconds: Double, line: String)] = [
        (0.0, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app -> locked, notify true for 0.0 s"),
        (0.8, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app"),
        (1.4, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app"),
        (2.0, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app"),
        (2.6, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app"),
        (8.7, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=true, front=SpringBoard -> locked, notify true for 0.0 s"),
    ]

    /// The run-137 lock-mid-task lines (`LockedPhoneAgentTests`): unlocked, the lock, 3.9 s
    /// later still locked, the unlock.
    static let run137LockMidTask: [(seconds: Double, line: String)] = [
        (0.0, "lock signals: notify(com.apple.springboard.lockstate)=false, lock screen=not checked, front=dev.jarvis.app -> unlocked"),
        (10.0, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app -> locked, notify true for 0.0 s"),
        (13.9, "lock signals: notify(com.apple.springboard.lockstate)=true, lock screen=not checked, front=dev.jarvis.app -> locked, notify true for 3.9 s"),
        (20.0, "lock signals: notify(com.apple.springboard.lockstate)=false, lock screen=not checked, front=dev.jarvis.app -> unlocked"),
    ]

    /// A reading back from its line (`LockSignals.description`), and the verdict the line
    /// recorded, if any.
    static func reading(_ line: String) -> (LockSignals, DeviceLockState?)? {
        func field(_ key: String) -> String? {
            guard let r = line.range(of: key) else { return nil }
            let rest = line[r.upperBound...]
            let end = rest.firstIndex { $0 == "," || $0 == " " } ?? rest.endIndex
            return String(rest[..<end])
        }
        func tri(_ v: String?) -> Bool?? {
            switch v {
            case "true": return .some(true)
            case "false": return .some(false)
            case "unavailable", "not": return .some(nil)
            default: return nil
            }
        }
        guard let notify = tri(field("notify(com.apple.springboard.lockstate)=")),
              let lockScreen = tri(field("lock screen=")),
              let front = field("front=") else { return nil }
        let signals = LockSignals(notify: notify, lockScreen: lockScreen,
                                  front: front == "SpringBoard" ? .springBoard : .app(front))
        let verdict = line.range(of: "-> ").map { r in DeviceLockState(rawValue: String(line[r.upperBound...].prefix { $0 != "," })) } ?? nil
        return (signals, verdict)
    }

    @Test("the run-137 lines parse back into the readings they print")
    func linesRoundTrip() throws {
        for (_, line) in Self.run137LockProbe + Self.run137LockMidTask {
            let (signals, _) = try #require(Self.reading(line), "\(line)")
            #expect(line.hasPrefix(signals.description), "\(line)")
        }
    }

    @Test("run 137, the lock probe: locked from the lock button on, with the locked app reading as in front, and when the app keeps reading so past 12 s (a device: audio, navigation)")
    func run137LockProbeSequence() throws {
        let judge = LockJudge()
        for (t, line) in Self.run137LockProbe {
            let (signals, recorded) = try #require(Self.reading(line))
            let v = judge.judge(signals, at: .seconds(t))
            #expect(v.state == .locked, "t+\(t)s \(line)")
            if let recorded { #expect(v.state == recorded, "t+\(t)s: the runner recorded \(recorded)") }
        }
        // The C6 case: the first reading (the app in front, the lock screen not checked)
        // repeated well past the stale time. Old rule: unlocked at 12 s.
        let longer = LockJudge()
        let (first, _) = try #require(Self.reading(Self.run137LockProbe[0].line))
        for t in stride(from: 0.0, through: 30.0, by: 0.6) {
            #expect(longer.judge(first, at: .seconds(t)).state == .locked, "t+\(t)s")
        }
        // And with the check made (SpringBoard's tree read behind the app): it shows the
        // lock screen, locked; it shows none for 12 s (a simulator whose notify state is
        // stuck), stale, as before.
        let checked = LockJudge()
        let shown = LockSignals(notify: true, lockScreen: true, front: first.front)
        let notShown = LockSignals(notify: true, lockScreen: false, front: first.front)
        #expect(checked.judge(shown, at: .zero).state == .locked)
        #expect(checked.judge(shown, at: .seconds(20)).state == .locked)
        #expect(checked.judge(notShown, at: .seconds(31)).state == .locked)
        #expect(checked.judge(notShown, at: .seconds(32)).state == .unlocked)
    }

    @Test("run 137, lock mid-task: unlocked, locked from the lock, still locked 3.9 s later, unlocked after")
    func run137LockMidTaskSequence() throws {
        let judge = LockJudge()
        for (t, line) in Self.run137LockMidTask {
            let (signals, recorded) = try #require(Self.reading(line))
            let v = judge.judge(signals, at: .seconds(t))
            #expect(v.state == recorded, "t+\(t)s \(line)")
        }
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
