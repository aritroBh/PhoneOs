import Foundation

/// Waits for `task` at most `limit` and returns its value, or `nil` when the limit passes
/// first or the caller is cancelled. `task` itself is never cancelled: it keeps running and
/// whoever else awaits it still gets its result.
///
/// The App Intents use it: iOS ends an intent after about 30 s, so a long workflow is
/// answered "still running" while it goes on in Jarvis. (A task group over `task.value`
/// cannot do this: the group waits for every child, so it returns only when `task` ends.)
public func value<T: Sendable>(of task: Task<T, Never>, within limit: Duration) async -> T? {
    // The race's own operation task (which only awaits `task.value`) is the one cancelled;
    // awaiting `value` does not pass cancellation on to `task`.
    try? await IORace<T>().run(kind: .act, timeout: limit) { await task.value }
}
