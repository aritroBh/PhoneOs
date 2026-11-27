import Foundation

/// What reading one of Jarvis's JSON files found. The three cases are kept apart on purpose:
///
/// - `missing`: there is no file. Normal on first use: the store starts from its defaults.
/// - `unreadable`: the file is there but could not be read. On iOS this is every read before
///   the first unlock after a restart (the files are written with
///   `.completeFileProtectionUntilFirstUserAuthentication`), or a transient I/O error. The
///   file is probably fine: nothing may overwrite it, and the next call reads it again.
/// - `damaged`: the bytes were read but are not valid for the schema. The file was renamed
///   aside (`<name>.corrupt-<ms>`, never deleted, never overwritten) and `setAside` says
///   where it went; `nil` when it could not be renamed (then nothing may overwrite it).
public enum StoredFileRead<Value> {
    case loaded(Value)
    case missing
    case unreadable(reason: String)
    case damaged(reason: String, setAside: URL?)

    /// The value: loaded, or `fallback` when there is none to use.
    public func value(or fallback: @autoclosure () -> Value) -> Value {
        if case .loaded(let value) = self { return value }
        return fallback()
    }

    /// Whether writing a new file at that path keeps everything the owner had: true when
    /// the file was loaded, missing, or damaged and set aside.
    public var mayWrite: Bool {
        switch self {
        case .loaded, .missing: true
        case .unreadable: false
        case .damaged(_, let setAside): setAside != nil
        }
    }
}

extension StoredFileRead: Sendable where Value: Sendable {}
extension StoredFileRead: Equatable where Value: Equatable {}

/// A store refused to write because the file on disk could not be read (or was damaged and
/// could not be set aside): writing would overwrite what the owner had.
public enum StoredFileError: Error, Sendable, Equatable, CustomStringConvertible {
    case unreadable(file: String, reason: String)
    case damagedNotSetAside(file: String, reason: String)

    public var description: String {
        switch self {
        case .unreadable(let file, let reason):
            "\(file) could not be read (\(reason)); it was not changed, so nothing in it is lost. Try again once the phone has been unlocked."
        case .damagedNotSetAside(let file, let reason):
            "\(file) is damaged (\(reason)) and could not be set aside; it was not changed."
        }
    }
}

/// The file rules every JSON store here shares (the same as `SkillLibrary` and
/// `WorkflowLibrary`): read with the three outcomes above, rename a damaged file aside.
enum StoredFile {
    /// Reads `url` and decodes it with `decode`. A damaged file is renamed aside.
    static func read<Value>(_ url: URL, now: () -> Date = Date.init,
                            decode: (Data) throws -> Value) -> StoredFileRead<Value> {
        let data: Data
        switch readData(url) {
        case .missing: return .missing
        case .unreadable(let reason): return .unreadable(reason: reason)
        case .damaged(let reason, let setAside): return .damaged(reason: reason, setAside: setAside)
        case .loaded(let bytes): data = bytes
        }
        do {
            return .loaded(try decode(data))
        } catch {
            let reason = describe(error)
            return .damaged(reason: reason, setAside: try? setAside(url, now: now))
        }
    }

    /// The bytes of `url`: `missing` only when there is no file at all.
    static func readData(_ url: URL) -> StoredFileRead<Data> {
        do {
            return .loaded(try Data(contentsOf: url))
        } catch {
            if isNoSuchFile(error), !FileManager.default.fileExists(atPath: url.path) { return .missing }
            return .unreadable(reason: describe(error))
        }
    }

    /// Renames `url` to `<name>.corrupt-<ms>` (with `-n` when taken) and returns the new URL.
    static func setAside(_ url: URL, now: () -> Date) throws -> URL {
        let fm = FileManager.default
        let stamp = (now().timeIntervalSince1970 * 1_000).clampedInt
        var target = url.appendingPathExtension("corrupt-\(stamp)")
        var n = 1
        while fm.fileExists(atPath: target.path) {
            target = url.appendingPathExtension("corrupt-\(stamp)-\(n)")
            n += 1
        }
        try fm.moveItem(at: url, to: target)
        return target
    }

    /// Throws unless `read` allows writing a new file in place of the one it read.
    static func requireWritable<Value>(_ read: StoredFileRead<Value>, _ url: URL) throws {
        switch read {
        case .loaded, .missing: return
        case .unreadable(let reason):
            throw StoredFileError.unreadable(file: url.lastPathComponent, reason: reason)
        case .damaged(let reason, let setAside):
            if setAside == nil { throw StoredFileError.damagedNotSetAside(file: url.lastPathComponent, reason: reason) }
        }
    }

    /// Writes `data` atomically, protected until the first unlock on Apple platforms (the
    /// same protection as `FileMemoryStore`).
    static func write(_ data: Data, to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if canImport(Darwin)
        try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try data.write(to: url, options: [.atomic])
        #endif
    }

    /// Owner-facing one-line text of a read or decode error.
    static func describe(_ error: any Error) -> String {
        if let decoding = error as? DecodingError {
            switch decoding {
            case .dataCorrupted(let c), .keyNotFound(_, let c), .typeMismatch(_, let c), .valueNotFound(_, let c):
                return "not valid JSON for this file: \(c.debugDescription)"
            @unknown default:
                return "not valid JSON for this file"
            }
        }
        let ns = error as NSError
        if ns.domain == NSCocoaErrorDomain, let underlying = ns.userInfo[NSUnderlyingErrorKey] as? NSError {
            return "\(ns.localizedDescription) (\(underlying.domain) \(underlying.code))"
        }
        return ns.localizedDescription
    }

    private static func isNoSuchFile(_ error: any Error) -> Bool {
        let ns = error as NSError
        if ns.domain == NSCocoaErrorDomain, ns.code == CocoaError.fileReadNoSuchFile.rawValue || ns.code == CocoaError.fileNoSuchFile.rawValue {
            return true
        }
        if ns.domain == NSPOSIXErrorDomain, ns.code == Int(ENOENT) { return true }
        if let underlying = ns.userInfo[NSUnderlyingErrorKey] as? NSError,
           underlying.domain == NSPOSIXErrorDomain, underlying.code == Int(ENOENT) {
            return true
        }
        return false
    }
}
