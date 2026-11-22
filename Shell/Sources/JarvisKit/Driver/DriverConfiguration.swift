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
    /// Pause between lock-state polls while the session waits for an unlock (each poll
    /// answers `Effect.awaitUnlock` with `tick`, `deviceUnlocked` or an observation).
    public var unlockPollInterval: Duration
    /// How often the runner's lock state is checked while the driver waits for the model,
    /// a tool or the owner; a lock interrupts the wait with `Event.deviceLocked`. `nil`
    /// turns the watch off.
    public var lockWatchInterval: Duration?
    /// The Live view's real-screen option ("Show real screen in Live view", off by default).
    /// It never changes the run: the core's observation is taken exactly as with the option
    /// off. While the option is on and `DriverDependencies.viewerFrames` is enabled (checked
    /// on every observation, so turning it off mid-run stops it at once), an observation the
    /// core asked no pixels for is followed by a separate best-effort
    /// `PhoneRunner.viewerScreenshot()` with its own deadline (`viewerScreenshotTimeout`);
    /// pixels the core asked for are reused. Errors and timeouts are ignored (one `.note`
    /// per run at most), never retried, and count toward no failure, deadline or effect
    /// budget. Fail closed on the lock: pixels only for an observation the runner reported
    /// unlocked, with the runner still reporting unlocked after the capture, never in or
    /// out of an unlock wait, and dropped when the core answers the observation with an
    /// unlock wait. Frames are kept in `viewerFrames` (memory only). Without
    /// `viewerFrames` this does nothing.
    public var viewerScreenshots: Bool
    /// Deadline for one Live view capture (the screenshot plus the lock check after it),
    /// and for one approval capture (`approvalScreenshots`).
    public var viewerScreenshotTimeout: Duration
    /// The approval sheet's real-screen option ("Show the real screen when Jarvis asks";
    /// the app's setting is on by default, this driver default is off so a driver built
    /// without it captures nothing). It never changes the run: when the core asks the owner
    /// to confirm (`Effect.confirm`), and only then, the driver makes one best-effort
    /// capture of the screen the confirmation was computed from, bounded by
    /// `viewerScreenshotTimeout`, before the question goes out. The frame is bound to that
    /// observation (`ViewerFrameKey`): the observation's own pixels when the core asked for
    /// them, else the Live view's frame of it, else `PhoneRunner.viewerScreenshot()`
    /// followed by a tree read that must show the very screen evaluated (same tree, app,
    /// size, keyboard and orientation) and report the phone unlocked; a screen that moved
    /// on keeps nothing. Fail closed on the lock: only after an observation the runner
    /// reported unlocked, never in an unlock wait. Errors and timeouts are ignored (no note,
    /// no IO failure, never retried). The frame goes to `DriverDependencies.approvalFrames`
    /// (memory only) and is removed as soon as the question is resolved, whatever resolved
    /// it. Without `approvalFrames`, or while it does not accept frames, nothing is
    /// captured at all.
    public var approvalScreenshots: Bool

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
        updateBufferSize: Int = 1_024,
        unlockPollInterval: Duration = .seconds(2),
        lockWatchInterval: Duration? = .seconds(3),
        viewerScreenshots: Bool = false,
        viewerScreenshotTimeout: Duration = .seconds(1),
        approvalScreenshots: Bool = false
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
        self.unlockPollInterval = unlockPollInterval
        self.lockWatchInterval = lockWatchInterval
        self.viewerScreenshots = viewerScreenshots
        self.viewerScreenshotTimeout = viewerScreenshotTimeout
        self.approvalScreenshots = approvalScreenshots
    }
}
