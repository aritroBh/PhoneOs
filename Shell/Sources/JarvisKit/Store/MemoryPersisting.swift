import Foundation

/// Where the session's memory JSON goes when a run ends.
///
/// `SessionDriver` calls `save` exactly once per run, on every exit path (success, failure,
/// cancellation, IO errors), with the string `JarvisSession.finish()` returned. It is not
/// called when `finish()` throws (the core's memory may be half-updated after an internal
/// error; keep the previous bytes).
public protocol MemoryPersisting: Sendable {
    /// Persists the memory JSON (replacing what was there).
    func save(_ memoryJSON: String) async throws
}
