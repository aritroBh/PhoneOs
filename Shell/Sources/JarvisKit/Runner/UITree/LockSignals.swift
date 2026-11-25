import Foundation
import Synchronization

/// The raw lock readings a runner without a lock API has. They are only readings: the
/// decision is `LockJudge`'s, one verdict for commands and for resuming alike.
///
/// Readings (both may be wrong or missing on a given iOS or simulator, which is why both are
/// recorded and neither alone may latch a session):
/// - `notify`: the Darwin notify state of `com.apple.springboard.lockstate` (`nil`: not
///   readable). It can stay stale in the simulator.
/// - `lockScreen`: SpringBoard's tree matches `LockScreenSignature` (`nil`: not checked,
///   for example because the snapshot failed, or an app is in front and the notify state
///   does not say locked). The XCUITest runner checks SpringBoard's tree whenever
///   SpringBoard is in front, and also with an app in front once the notify state says
///   locked (audit C6).
/// - `front`: what is in front.
///
/// There used to be two rules over these (commands: either says locked; resume: unlocked as
/// soon as an app is in front). They contradicted each other whenever `notify` said locked
/// with an app in front: the command was refused, the unlock wait ended at once, the command
/// was refused again, and the task's retries went in a few seconds (run on c0cff3e,
/// `testLockMidTaskWaitsAndResumesAfterUnlock`). Never combine raw rules again: staleness of
/// `notify` is decided by time, in `LockJudge`, once.
public struct LockSignals: Sendable, Equatable, CustomStringConvertible {
    public enum Front: Sendable, Equatable {
        /// An app (not SpringBoard) is in front.
        case app(String)
        /// SpringBoard is in front (home screen or lock screen).
        case springBoard
    }

    public var notify: Bool?
    public var lockScreen: Bool?
    public var front: Front

    public init(notify: Bool?, lockScreen: Bool?, front: Front) {
        self.notify = notify
        self.lockScreen = lockScreen
        self.front = front
    }

    /// The raw readings (the verdict is in `LockVerdict.description`).
    public var description: String {
        let frontText: String
        switch front {
        case .app(let id): frontText = id
        case .springBoard: frontText = "SpringBoard"
        }
        return "lock signals: notify(com.apple.springboard.lockstate)=\(notify.map(String.init) ?? "unavailable"), "
            + "lock screen=\(lockScreen.map(String.init) ?? "not checked"), front=\(frontText)"
    }
}

/// What `LockJudge` decided for one reading.
public struct LockVerdict: Sendable, Equatable, CustomStringConvertible {
    /// The one answer, for refusing a command and for `PhoneRunner.lockState()` alike.
    public var state: DeviceLockState
    /// How long the notify reading has been continuously true (`nil`: it is not true now).
    public var notifyTrueFor: Duration?
    /// Set on the one reading at which the notify state was declared stale (for the
    /// observation notes).
    public var staleNote: String?

    public init(state: DeviceLockState, notifyTrueFor: Duration? = nil, staleNote: String? = nil) {
        self.state = state
        self.notifyTrueFor = notifyTrueFor
        self.staleNote = staleNote
    }

    public var description: String {
        let held = notifyTrueFor.map { ", notify true for \(Self.seconds($0))" } ?? ""
        return "-> \(state.rawValue)\(held)"
    }

    static func seconds(_ d: Duration) -> String {
        let c = d.components
        let s = Double(c.seconds) + Double(c.attoseconds) / 1e18
        return String(format: "%.1f s", s)
    }
}

/// Judges `LockSignals` over time: one verdict used for commands and for resume, so the two
/// can never disagree.
///
/// - the lock screen is confirmed: locked;
/// - the notify state says locked and the lock screen was not checked (`nil`: the snapshot
///   failed, or a runner that does not check): locked, however long it lasts — the notify
///   state never goes stale on such a reading (audit C6, run 137: right after the lock the
///   locked app still read as in front; a runner checks SpringBoard's tree then);
/// - the notify state says locked and the check showed no lock screen (`false`; an app or
///   SpringBoard in front): locked while the notify state has been true for less than
///   `notifyStaleAfter`; after that, with the lock screen never confirmed in the stretch, it
///   is declared stale (once, with a note) and such readings are unlocked until the notify
///   state reads false. A confirmed lock screen restarts the clock (it was real);
/// - the notify state says unlocked: unlocked;
/// - no notify reading: the lock screen reading decides; neither: unknown.
///
/// Time is passed in (any origin, never decreasing); the judge reads no clock.
public final class LockJudge: Sendable {
    public static let defaultNotifyStaleAfter: Duration = .seconds(12)

    private struct State {
        var notifyTrueSince: Duration?
        var stale = false
    }

    public let notifyStaleAfter: Duration
    private let state = Mutex(State())

    public init(notifyStaleAfter: Duration = LockJudge.defaultNotifyStaleAfter) {
        self.notifyStaleAfter = notifyStaleAfter
    }

    public func judge(_ signals: LockSignals, at now: Duration) -> LockVerdict {
        state.withLock { s -> LockVerdict in
            if signals.notify != true {
                s.notifyTrueSince = nil
                s.stale = false
                if signals.lockScreen == true { return LockVerdict(state: .locked) }
                if signals.notify == false || signals.lockScreen == false { return LockVerdict(state: .unlocked) }
                return LockVerdict(state: .unknown)
            }
            if s.notifyTrueSince == nil { s.notifyTrueSince = now }
            if signals.lockScreen == true {
                s.notifyTrueSince = now
                s.stale = false
                return LockVerdict(state: .locked, notifyTrueFor: .zero)
            }
            let held = max(.zero, now - (s.notifyTrueSince ?? now))
            // Audit C6 (run 137): notify may go stale only on a reading whose lock screen
            // check positively showed none. Unchecked (an app reads as in front, the
            // snapshot failed) it stays locked however long it lasts: right after the lock
            // the locked app kept reading as foreground for about 8 s on the simulator, and
            // on a device (background audio, navigation) it can be longer than 12 s.
            if signals.lockScreen == nil { return LockVerdict(state: .locked, notifyTrueFor: held) }
            if s.stale { return LockVerdict(state: .unlocked, notifyTrueFor: held) }
            if held >= notifyStaleAfter {
                s.stale = true
                let note = "the lock notification has said locked for \(LockVerdict.seconds(notifyStaleAfter)) with no lock screen; treated as stale"
                return LockVerdict(state: .unlocked, notifyTrueFor: held, staleNote: note)
            }
            return LockVerdict(state: .locked, notifyTrueFor: held)
        }
    }
}
