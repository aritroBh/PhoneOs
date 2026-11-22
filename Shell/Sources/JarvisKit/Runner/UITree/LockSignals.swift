import Foundation

/// The lock readings a runner without a lock API has, and the decision rule for each use.
///
/// Readings (both may be wrong or missing on a given iOS or simulator, which is why both are
/// recorded and neither alone may latch a session):
/// - `notify`: the Darwin notify state of `com.apple.springboard.lockstate` (`nil`: not
///   readable). It can stay stale in the simulator.
/// - `lockScreen`: SpringBoard's tree matches `LockScreenSignature` (`nil`: not checked,
///   for example because the snapshot failed).
/// - `front`: what is in front.
///
/// Rules:
/// - **Refusing a command** (`forCommands`): locked when either reading says locked. Acting
///   on a phone that might be locked is the costly mistake.
/// - **Resuming or watching** (`forResume`, what `PhoneRunner.lockState()` answers): unlocked
///   as soon as an app is in front, or SpringBoard is in front without the lock screen (the
///   home screen), even if the notify state still says locked; the notify state decides only
///   when the lock screen could not be checked. A stale notify state therefore cannot hold
///   a session in its unlock wait.
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

    /// Whether to refuse a command: either reading says locked.
    public var forCommands: DeviceLockState {
        if notify == true || effectiveLockScreen == true { return .locked }
        if notify != nil || effectiveLockScreen != nil { return .unlocked }
        return .unknown
    }

    /// Whether the session may resume (and what the lock watch reacts to).
    public var forResume: DeviceLockState {
        switch front {
        case .app:
            return .unlocked
        case .springBoard:
            if let lockScreen { return lockScreen ? .locked : .unlocked }
            switch notify {
            case true?: return .locked
            case false?: return .unlocked
            case nil: return .unknown
            }
        }
    }

    /// The lock screen reading with an app in front counted as "not the lock screen".
    private var effectiveLockScreen: Bool? {
        if case .app = front { return lockScreen ?? false }
        return lockScreen
    }

    /// Both raw readings and both decisions, for notes and failure messages.
    public var description: String {
        let frontText: String
        switch front {
        case .app(let id): frontText = id
        case .springBoard: frontText = "SpringBoard"
        }
        return "lock signals: notify(com.apple.springboard.lockstate)=\(notify.map(String.init) ?? "unavailable"), "
            + "lock screen=\(lockScreen.map(String.init) ?? "not checked"), front=\(frontText) "
            + "-> commands \(forCommands.rawValue), resume \(forResume.rawValue)"
    }
}
