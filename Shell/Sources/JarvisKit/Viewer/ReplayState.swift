import Foundation

/// The Live view's scrubber: which step is shown, live or not, playing or paused, at what
/// speed, and when the shown step's animation started. Pure: every change takes the step
/// numbers the timeline holds now (`ViewerTimeline.stepNumbers`, oldest first) and the
/// current time in seconds (any monotonic clock), so it is tested without a clock.
///
/// - **Live** (the default): follows the newest step; each new step starts its animation.
/// - Stepping, scrubbing or playing leaves live. New steps arriving then never move the
///   shown step; if the cap drops the shown step, the oldest kept step is shown.
/// - `goLive()` returns to following.
public struct ReplayState: Sendable, Equatable {
    public enum Speed: Double, Sendable, Equatable, CaseIterable {
        case normal = 1
        case double = 2

        public var title: String {
            switch self {
            case .normal: "1×"
            case .double: "2×"
            }
        }

        public var next: Speed { self == .normal ? .double : .normal }
    }

    /// The shown step's number, `nil` while there are no steps.
    public private(set) var current: Int?
    /// Following the newest step.
    public private(set) var isFollowing = true
    /// Replay is playing (never while following).
    public private(set) var isPlaying = false
    public private(set) var speed: Speed = .normal
    /// When the shown step's animation started (seconds).
    public private(set) var startedAt: Double = 0

    public init() {}

    /// Milliseconds into the shown step's animation at `now` (scaled by the speed in replay).
    public func elapsedMs(now: Double) -> Double {
        let seconds = now.isFinite && startedAt.isFinite ? max(0, now - startedAt) : 0
        return seconds * 1_000 * (isFollowing ? 1 : speed.rawValue)
    }

    /// The timeline changed (a step was added, finished or dropped).
    public mutating func sync(numbers: [Int], now: Double) {
        guard let last = numbers.last, let first = numbers.first else {
            current = nil
            isPlaying = false
            return
        }
        if isFollowing {
            if current != last { show(last, now: now) }
            return
        }
        guard let shown = current else {
            show(first, now: now)
            return
        }
        // Dropped by the cap: the oldest kept step.
        if !numbers.contains(shown) { show(first, now: now) }
    }

    /// One step back (leaves live, pauses).
    public mutating func stepBack(numbers: [Int], now: Double) {
        guard let i = index(in: numbers) else { return }
        isFollowing = false
        isPlaying = false
        show(numbers[max(0, i - 1)], now: now)
    }

    /// One step forward (leaves live, pauses). At the newest step it stays there.
    public mutating func stepForward(numbers: [Int], now: Double) {
        guard let i = index(in: numbers) else { return }
        isFollowing = false
        isPlaying = false
        show(numbers[min(numbers.count - 1, i + 1)], now: now)
    }

    /// Shows the step numbered `number` (leaves live, pauses). Unknown numbers are ignored.
    public mutating func scrub(to number: Int, numbers: [Int], now: Double) {
        guard numbers.contains(number) else { return }
        isFollowing = false
        isPlaying = false
        show(number, now: now)
    }

    /// Shows the step at `fraction` (0…1) of the kept steps (the scrubber's value).
    public mutating func scrub(fraction: Double, numbers: [Int], now: Double) {
        guard !numbers.isEmpty else { return }
        let f = fraction.isFinite ? min(max(fraction, 0), 1) : 0
        let i = Int((f * Double(numbers.count - 1)).rounded())
        scrub(to: numbers[min(max(i, 0), numbers.count - 1)], numbers: numbers, now: now)
    }

    /// The scrubber's value for the shown step (0…1; 1 while following).
    public func fraction(numbers: [Int]) -> Double {
        guard numbers.count > 1, let i = index(in: numbers) else { return 1 }
        return Double(i) / Double(numbers.count - 1)
    }

    /// Play / pause. Playing leaves live; at the newest step it starts again from the oldest.
    public mutating func togglePlay(numbers: [Int], now: Double) {
        guard !numbers.isEmpty else { return }
        if isPlaying {
            isPlaying = false
            return
        }
        isFollowing = false
        isPlaying = true
        if current == nil || current == numbers.last {
            show(numbers[0], now: now)
        } else if let current {
            show(current, now: now)
        }
    }

    public mutating func setSpeed(_ speed: Speed, now: Double) {
        guard speed != self.speed else { return }
        // Keep the animation where it is at the new speed.
        if !isFollowing, speed.rawValue > 0 {
            let elapsed = elapsedMs(now: now)
            startedAt = now - elapsed / 1_000 / speed.rawValue
        }
        self.speed = speed
    }

    /// Back to following the newest step.
    public mutating func goLive(numbers: [Int], now: Double) {
        isFollowing = true
        isPlaying = false
        if let last = numbers.last {
            if current != last { show(last, now: now) }
        } else {
            current = nil
        }
    }

    /// While playing: moves to the next step once the shown one has played for
    /// `playMs(current)` milliseconds; stops at the newest step. Returns whether the shown
    /// step changed.
    @discardableResult
    public mutating func advance(numbers: [Int], now: Double, playMs: (Int) -> Double) -> Bool {
        guard isPlaying, let shown = current, let i = numbers.firstIndex(of: shown) else { return false }
        guard elapsedMs(now: now) >= max(0, playMs(shown)) else { return false }
        if i + 1 < numbers.count {
            show(numbers[i + 1], now: now)
            return true
        }
        isPlaying = false
        return false
    }

    private func index(in numbers: [Int]) -> Int? {
        guard !numbers.isEmpty else { return nil }
        if let current, let i = numbers.firstIndex(of: current) { return i }
        return numbers.count - 1
    }

    private mutating func show(_ number: Int, now: Double) {
        current = number
        startedAt = now.isFinite ? now : 0
    }
}
