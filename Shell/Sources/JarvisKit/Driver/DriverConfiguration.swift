import Foundation

/// Deadlines, retries and caps for `SessionDriver`. The defaults suit a phone: generous for
/// on-device inference, short for runner calls.
public struct DriverConfiguration: Sendable, Equatable {
    /// Deadline for one runner snapshot (`Effect.observe`).
    public var observeTimeout: Duration
    /// Attempts per `Effect.observe` before the runner counts as unavailable (at least 1).
    public var observeAttempts: Int
    /// Pause between observe attempts.
    public var observeRetryDelay: Duration
    /// Deadline for one runner command (`Effect.act`).
    public var actTimeout: Duration
    /// Deadline for one model call (`Effect.infer`).
    public var inferTimeout: Duration
    /// Deadline for one tool call (`Effect.callTool`).
    public var toolTimeout: Duration
    /// Deadline for the owner to confirm or answer; `nil` waits as long as they take.
    public var humanTimeout: Duration?
    /// Deadline for `MemoryPersisting.save`.
    public var memorySaveTimeout: Duration
    /// Effects after which the driver cancels the session (a backstop under the core's own
    /// step and time budgets).
    public var maxEffects: Int
    /// Updates buffered for a slow `updates` consumer (newest kept).
    public var updateBufferSize: Int

    /// The defaults, each overridable.
    public init(
        observeTimeout: Duration = .seconds(20),
        observeAttempts: Int = 3,
        observeRetryDelay: Duration = .milliseconds(500),
        actTimeout: Duration = .seconds(20),
        inferTimeout: Duration = .seconds(180),
        toolTimeout: Duration = .seconds(30),
        humanTimeout: Duration? = .seconds(600),
        memorySaveTimeout: Duration = .seconds(10),
        maxEffects: Int = 2_000,
        updateBufferSize: Int = 1_024
    ) {
        self.observeTimeout = observeTimeout
        self.observeAttempts = observeAttempts
        self.observeRetryDelay = observeRetryDelay
        self.actTimeout = actTimeout
        self.inferTimeout = inferTimeout
        self.toolTimeout = toolTimeout
        self.humanTimeout = humanTimeout
        self.memorySaveTimeout = memorySaveTimeout
        self.maxEffects = maxEffects
        self.updateBufferSize = updateBufferSize
    }
}
