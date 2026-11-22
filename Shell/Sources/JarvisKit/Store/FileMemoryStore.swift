import Foundation
import Dispatch

/// Errors of `FileMemoryStore`.
public enum MemoryStoreError: Error, Equatable, Sendable, CustomStringConvertible {
    /// Reading, writing or renaming failed.
    case io(String)

    public var description: String {
        switch self {
        case .io(let message): "memory store: \(message)"
        }
    }
}

/// The session memory JSON in one file.
///
/// - Writes are atomic (temporary file, then rename), so a crash never leaves half a file.
/// - A file that is not a JSON object is renamed aside (`<name>.corrupt-<timestamp>`) and
///   `load()` returns `""` (fresh memory) instead of failing; call `quarantine()` yourself
///   when the core rejects bytes that are valid JSON (an incompatible schema).
/// - On Apple platforms the file gets complete-until-first-user-authentication data
///   protection, so the background agent can read it while the phone is locked after the
///   first unlock.
/// - All file work runs on a private serial queue, never on the Swift concurrency pool.
public final class FileMemoryStore: MemoryPersisting {
    /// The memory file.
    public let fileURL: URL
    private let queue: DispatchQueue
    private let now: @Sendable () -> Date

    /// A store for `fileURL`; its directory is created on the first save. `now` names
    /// quarantined files (injectable for tests).
    public init(fileURL: URL, now: @escaping @Sendable () -> Date = Date.init) {
        self.fileURL = fileURL
        self.queue = DispatchQueue(label: "JarvisKit.FileMemoryStore")
        self.now = now
    }

    /// The saved memory JSON, or `""` when there is none or the file was corrupt (and has
    /// been quarantined).
    public func load() async throws -> String {
        try await onQueue { try self.loadSync() }
    }

    /// The saved memory JSON with which case the file was in: `missing`, `loaded`,
    /// `unreadable` (the file is there but could not be read; it is left untouched) or
    /// `damaged` (not a JSON object; renamed aside, `setAside` says where). Never throws.
    public func read() async -> StoredFileRead<String> {
        do {
            return try await onQueue { self.readSync() }
        } catch {
            return .unreadable(reason: String(describing: error))
        }
    }

    /// Writes `memoryJSON` atomically, replacing the previous file.
    public func save(_ memoryJSON: String) async throws {
        try await onQueue { try self.saveSync(memoryJSON) }
    }

    /// Renames the current file aside (if any) and returns the quarantine file's URL.
    @discardableResult
    public func quarantine() async throws -> URL? {
        try await onQueue { try self.quarantineSync() }
    }

    // MARK: - Synchronous parts (on `queue` only)

    private func loadSync() throws -> String {
        switch readSync() {
        case .loaded(let text): return text
        case .missing: return ""
        case .unreadable(let reason): throw MemoryStoreError.io("cannot read \(fileURL.path): \(reason)")
        case .damaged(let reason, let setAside):
            if setAside == nil { throw MemoryStoreError.io("cannot quarantine \(fileURL.path): \(reason)") }
            return ""
        }
    }

    private func readSync() -> StoredFileRead<String> {
        let data: Data
        switch StoredFile.readData(fileURL) {
        case .missing: return .missing
        case .unreadable(let reason): return .unreadable(reason: reason)
        case .damaged(let reason, let setAside): return .damaged(reason: reason, setAside: setAside)
        case .loaded(let bytes): data = bytes
        }
        let isObject = (try? JSONSerialization.jsonObject(with: data)) is [String: Any]
        guard isObject, let text = String(data: data, encoding: .utf8) else {
            do {
                return .damaged(reason: "not a JSON object", setAside: try quarantineSync())
            } catch {
                return .damaged(reason: "not a JSON object; \(error)", setAside: nil)
            }
        }
        return .loaded(text)
    }

    private func saveSync(_ memoryJSON: String) throws {
        let fm = FileManager.default
        let directory = fileURL.deletingLastPathComponent()
        do {
            try fm.createDirectory(at: directory, withIntermediateDirectories: true)
            try Data(memoryJSON.utf8).write(to: fileURL, options: Self.writeOptions)
        } catch {
            throw MemoryStoreError.io("cannot write \(fileURL.path): \(error)")
        }
    }

    private func quarantineSync() throws -> URL? {
        let fm = FileManager.default
        guard fm.fileExists(atPath: fileURL.path) else { return nil }
        let stamp = (now().timeIntervalSince1970 * 1_000).clampedInt
        var target = fileURL.appendingPathExtension("corrupt-\(stamp)")
        var n = 1
        while fm.fileExists(atPath: target.path) {
            target = fileURL.appendingPathExtension("corrupt-\(stamp)-\(n)")
            n += 1
        }
        do {
            try fm.moveItem(at: fileURL, to: target)
        } catch {
            throw MemoryStoreError.io("cannot quarantine \(fileURL.path): \(error)")
        }
        return target
    }

    private static var writeOptions: Data.WritingOptions {
        #if canImport(Darwin)
        // Readable by the background agent after the first unlock; encrypted before it.
        // (The option exists from iOS 4 / macOS 11, below this package's minimums.)
        return [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
        #else
        return [.atomic]
        #endif
    }

    private func onQueue<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result { try work() })
            }
        }
    }
}
