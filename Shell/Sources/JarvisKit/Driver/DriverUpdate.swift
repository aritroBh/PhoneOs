import Foundation
import JarvisCore

/// One thing that happened during a run, for the UI, the action log and the Live Activity.
///
/// Order within a run: `.started` first; then for every effect `.effect`, `.progress`, and
/// unless the effect is `.finished`, the effect's own updates (`.confirmationPending`,
/// `.questionPending`, `.awaitingUnlock`, `.lockDetected`, `.ioFailed`, `.note`) and
/// `.effectCompleted`; `.unlockWaitEnded` comes right after the `.effect` that ends a wait (or
/// before `.finished`); `.finished` last, after
/// which the stream ends.
public enum DriverUpdate: Sendable, Equatable {
    /// The run began.
    case started(TaskSpec)
    /// The core asked for `effect`; `number` counts effects from 1.
    case effect(Effect, number: Int)
    /// The session's progress after that step.
    case progress(SessionProgress)
    /// A confirmation is waiting for the owner.
    case confirmationPending(ConfirmRequest)
    /// A question is waiting for the owner.
    case questionPending(String)
    /// One IO attempt failed (`willRetry` when the driver tries again).
    case ioFailed(IOKind, message: String, willRetry: Bool)
    /// Something worth logging that is not a failure: a runner adapter assumption, a
    /// runner-text parse issue reported by the core.
    case note(String)
    /// The session waits for the owner to unlock the phone (every `Effect.awaitUnlock`,
    /// with the time waited so far).
    case awaitingUnlock(reason: String, waitedMs: UInt64)
    /// The driver noticed a lock while it waited for the model, a tool or the owner, and
    /// abandoned that wait (`Event.deviceLocked` goes to the core).
    case lockDetected(IOKind)
    /// A wait for an unlock ended (the next effect is not `awaitUnlock`, or the run ended).
    case unlockWaitEnded
    /// The effect was carried out and `event` goes back to the core.
    case effectCompleted(Effect, event: Event)
    /// The run ended.
    case finished(RunResult)
}

/// The kinds of IO the driver performs (for deadlines and failure reports).
public enum IOKind: String, Sendable, Equatable, CaseIterable {
    case observe
    case act
    case infer
    case tool
    case confirm
    case answer
    case saveMemory
    /// A lock-state poll.
    case lockState
    /// Waiting between unlock polls.
    case awaitUnlock
    /// Telling the owner (`OwnerAttention`).
    case attention
}

/// How a run ended.
public struct RunResult: Sendable, Equatable {
    /// The core's outcome (status, summary, steps, duration).
    public var outcome: Outcome
    /// Whether the core finished on its own or the driver stopped it, and why.
    public var endReason: EndReason
    /// What happened to the session's memory.
    public var memory: MemorySaveStatus
    /// Effects the core emitted, including the final `.finished`.
    public var effectCount: Int
    /// `Effect.infer` effects carried out (model calls).
    public var modelCalls: Int

    /// Creates a result (the driver builds these; apps use it to report a run that could
    /// not start in the same shape).
    public init(outcome: Outcome, endReason: EndReason, memory: MemorySaveStatus, effectCount: Int, modelCalls: Int) {
        self.outcome = outcome
        self.endReason = endReason
        self.memory = memory
        self.effectCount = effectCount
        self.modelCalls = modelCalls
    }
}

/// Why a run ended. Every case except `.sessionFinished` means the driver delivered
/// `Event.cancel` to the core, so `outcome.status` is then `.cancelled`.
public enum EndReason: Sendable, Equatable {
    /// The core finished by itself (success, failure, blocked, budgets).
    case sessionFinished
    /// `SessionDriver.cancel()` or cancellation of the task running `run()`.
    case cancelled
    /// The owner could not be asked or did not answer in time.
    case noAnswer(String)
    /// `DriverConfiguration.maxEffects` was reached.
    case effectLimitReached(Int)
}

/// What happened to the memory JSON at the end of a run.
public enum MemorySaveStatus: Sendable, Equatable {
    /// `MemoryPersisting.save` succeeded.
    case saved
    /// `save` threw or timed out; the message says why.
    case saveFailed(String)
    /// `JarvisSession.finish()` threw (internal core error): nothing was saved, keep the
    /// previous bytes.
    case unavailable(String)
}
