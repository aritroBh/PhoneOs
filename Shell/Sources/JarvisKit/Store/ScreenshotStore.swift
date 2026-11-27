import Foundation
import JarvisCore
import Synchronization

/// Screenshots the driver took, keyed by the `ImageRef.id` the core sees. In memory only,
/// bounded to the `capacity` most recently stored or read (least recently used out first):
/// the core only ever refers to the latest screenshot or two.
public final class ScreenshotStore: Sendable {
    private struct Entry {
        var id: String
        var screenshot: CapturedScreenshot
    }

    private struct State {
        /// Most recently used last.
        var entries: [Entry] = []
        var nextNumber: UInt64 = 1
    }

    /// The maximum number of screenshots kept.
    public let capacity: Int
    private let state = Mutex(State())

    /// A store keeping at most `capacity` (at least 1) screenshots.
    public init(capacity: Int = 4) {
        self.capacity = max(1, capacity)
    }

    /// Stores `screenshot` under a new id and returns the handle for the core.
    public func store(_ screenshot: CapturedScreenshot) -> ImageRef {
        let id = state.withLock { state -> String in
            let id = "shot-\(state.nextNumber)"
            state.nextNumber += 1
            state.entries.append(Entry(id: id, screenshot: screenshot))
            if state.entries.count > capacity {
                state.entries.removeFirst(state.entries.count - capacity)
            }
            return id
        }
        return ImageRef(id: id, width: screenshot.pixelWidth, height: screenshot.pixelHeight)
    }

    /// The screenshot stored under `id` (and marks it most recently used), or `nil` when
    /// it was never stored or has been evicted.
    public func screenshot(for id: String) -> CapturedScreenshot? {
        state.withLock { state in
            guard let i = state.entries.firstIndex(where: { $0.id == id }) else { return nil }
            let entry = state.entries.remove(at: i)
            state.entries.append(entry)
            return entry.screenshot
        }
    }

    /// The ids currently held, least recently used first.
    public var storedIDs: [String] {
        state.withLock { $0.entries.map(\.id) }
    }

    /// Drops every screenshot.
    public func removeAll() {
        state.withLock { $0.entries.removeAll() }
    }
}
