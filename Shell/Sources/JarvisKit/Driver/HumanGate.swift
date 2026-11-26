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

    /// Review pass 35 (F4): asks as `confirm` does and says where the answer was given.
    /// The default (a gate that only implements `confirm`) reports it as not given on the
    /// phone: an answer is the phone's only when the gate knows it (a remote answer covers
    /// the action, a first use the task, and is never kept in memory).
    func decide(_ request: ConfirmRequest) async throws -> OwnerDecision

    /// Asks the owner a question and returns their answer.
    func answer(_ question: String) async throws -> String
}

/// An answer to a confirmation and where it was given (review pass 35, F4).
public struct OwnerDecision: Sendable, Equatable {
    /// `true` approves.
    public var approved: Bool
    /// The owner answered on the phone itself (Jarvis's own sheet); `false` for an answer
    /// relayed from a paired device or of unknown source.
    public var byPhone: Bool

    public init(approved: Bool, byPhone: Bool) {
        self.approved = approved
        self.byPhone = byPhone
    }
}

extension HumanGate {
    /// Unknown source: not the phone's.
    public func decide(_ request: ConfirmRequest) async throws -> OwnerDecision {
        OwnerDecision(approved: try await confirm(request), byPhone: false)
    }
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
