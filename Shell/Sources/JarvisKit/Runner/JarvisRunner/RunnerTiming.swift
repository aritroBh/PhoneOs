import Foundation
#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif

/// The clock both ends of Jarvis's runner protocol read for a request's `not_after_ms`
/// (protocol version 2, docs/JARVIS-RUNNER.md): milliseconds since boot, counting sleep.
///
/// - Apple platforms: `clock_gettime_nsec_np(CLOCK_MONOTONIC_RAW)`, i.e.
///   `mach_continuous_time` in nanoseconds. It is one kernel-wide counter that starts at
///   boot, never goes backwards and keeps counting while the phone sleeps; the app and the
///   runner process on the same phone read the same value at the same instant.
/// - Linux (tests): `clock_gettime(CLOCK_BOOTTIME)`, the same kind of clock.
///
/// The Rust channels read no clock; this value is passed in (the client's `not_after_ms`,
/// the server's `now_ms`).
public enum RunnerUptimeClock {
    public static func nowMs() -> UInt64 {
        #if canImport(Darwin)
        return clock_gettime_nsec_np(CLOCK_MONOTONIC_RAW) / 1_000_000
        #else
        var ts = timespec()
        guard clock_gettime(CLOCK_BOOTTIME, &ts) == 0 else { return 0 }
        return UInt64(max(0, ts.tv_sec)) &* 1_000 &+ UInt64(max(0, ts.tv_nsec)) / 1_000_000
        #endif
    }
}

/// The deadlines of the three layers over Jarvis's runner, derived from one place: the
/// driver's own deadlines (`DriverConfiguration()` defaults, which are not lengthened).
///
///     server deadline  <  client deadline  <  driver deadline
///     (16 s)              (18 s)              (20 s observe / 20 s act)
///
/// Each inner layer ends `margin` earlier than the one around it, so its answer (the
/// server's error reply, the client's `timedOut`) arrives before the outer layer gives up.
/// The client's deadline also covers connecting, and becomes the request's `not_after_ms`
/// (the server refuses the request when it arrives later).
/// `JarvisRunnerTests.timeoutLayering` asserts the order.
public enum JarvisRunnerTiming {
    /// Gap between two layers.
    public static let margin: Duration = .seconds(2)
    private static let driver = DriverConfiguration()

    /// The client's deadline for a read (snapshot, lock state, screenshot, front app).
    public static var clientRead: Duration { driver.observeTimeout - margin }
    /// The client's deadline for a command (plus its own duration for `wait`).
    public static var clientCommand: Duration { driver.actTimeout - margin }
    /// The server's deadline for a read.
    public static var serverRead: Duration { clientRead - margin }
    /// The server's deadline for a command (plus its own duration for `wait`).
    public static var serverCommand: Duration { clientCommand - margin }

    /// The server closes an authenticated connection that sends no request for this long,
    /// so a connection its app abandoned does not hold the only slot. Two minutes: an
    /// active run talks to the runner every few seconds (the lock watch polls every 3 s,
    /// unlock polls every 2 s), and the client retires its own connection after
    /// `clientIdleRetire`, before the server would, so this never cuts a connection the app
    /// is about to use; an idle or paused app simply connects again on its next call.
    public static let serverIdle: Duration = .seconds(120)
    /// The client opens a fresh connection instead of reusing one idle this long.
    public static let clientIdleRetire: Duration = .seconds(100)
}

extension Duration {
    /// Whole milliseconds, rounded down, never negative.
    var wholeMilliseconds: UInt64 {
        let (seconds, attoseconds) = components
        guard seconds >= 0 else { return 0 }
        return UInt64(seconds) &* 1_000 &+ UInt64(max(0, attoseconds) / 1_000_000_000_000_000)
    }
}
