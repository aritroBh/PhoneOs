import Foundation

/// A monotonic millisecond clock for `Event` timestamps (the core never reads a clock).
///
/// Named `MonotonicClock` rather than `Clock` so it does not shadow the standard library's
/// `Clock` protocol for code that imports JarvisKit.
public protocol MonotonicClock: Sendable {
    /// Milliseconds since an arbitrary fixed origin; never decreases.
    func nowMs() -> UInt64
}

/// `MonotonicClock` over `ContinuousClock` (keeps counting while the device sleeps, like
/// the time budget wants).
public struct SystemMonotonicClock: MonotonicClock {
    private let origin = ContinuousClock.now

    /// A clock whose zero is now.
    public init() {}

    public func nowMs() -> UInt64 {
        let elapsed = origin.duration(to: ContinuousClock.now)
        let (seconds, attoseconds) = elapsed.components
        let ms = seconds * 1_000 + attoseconds / 1_000_000_000_000_000
        return UInt64(max(0, ms))
    }
}
