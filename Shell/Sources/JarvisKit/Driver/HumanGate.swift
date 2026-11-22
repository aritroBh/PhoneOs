import Foundation
import JarvisCore

/// The owner, as the driver reaches them: confirmation sheets, spoken questions,
/// notifications while Jarvis is in the background.
///
/// Both methods may take as long as the owner takes; `DriverConfiguration.humanTimeout`
/// bounds the wait. Throwing (no UI available, notification dismissed) is a decline for
/// `confirm` and stops the run for `answer`.
public protocol HumanGate: Sendable {
    /// Asks the owner to approve a risky action. `true` approves.
    func confirm(_ request: ConfirmRequest) async throws -> Bool

    /// Asks the owner a question and returns their answer.
    func answer(_ question: String) async throws -> String
}

/// Getting the owner's attention when the run cannot go on without them physically: the
/// phone is locked and only they can unlock it (Face ID / passcode; Jarvis never tries).
///
/// The driver calls `awaitUnlock` once when a wait starts and `unlockWaitEnded` once when it
/// ends (unlocked, cancelled, interrupted or the run ended). Both must return promptly (post
/// a notification, update a Live Activity); the driver bounds them with a short deadline
/// and ignores failures.
public protocol OwnerAttention: Sendable {
    /// The session is waiting for an unlock (`reason` is the core's text).
    func awaitUnlock(reason: String, waitedMs: UInt64) async
    /// The wait is over.
    func unlockWaitEnded() async
}

/// No attention channel (tests, the app in the foreground).
public struct NoOwnerAttention: OwnerAttention {
    public init() {}
    public func awaitUnlock(reason: String, waitedMs: UInt64) async {}
    public func unlockWaitEnded() async {}
}
