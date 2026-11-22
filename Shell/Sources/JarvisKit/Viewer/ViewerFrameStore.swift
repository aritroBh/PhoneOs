import Foundation
import Synchronization

/// Names one observation of one run: the run's task id and the number of the driver effect
/// that observed (`DriverUpdate.effect(_, number:)` of the `observe`, or of the
/// `awaitUnlock` that observed). The driver stores a Live view frame under it and
/// `PointerTimeline` refers to it, so neither has to tell the other.
public struct ViewerFrameKey: Sendable, Equatable, Hashable, Codable {
    public var taskId: String
    public var observation: Int

    public init(taskId: String, observation: Int) {
        self.taskId = taskId
        self.observation = observation
    }

    enum CodingKeys: String, CodingKey {
        case taskId = "task_id"
        case observation
    }
}

/// The Live view's screenshots ("Show real screen in Live view"): **memory only**, never
/// written to disk, capped by count and by bytes, oldest out first. Separate from
/// `ScreenshotStore` (the model's), so the viewer can never change what the core is given
/// or evict what it still refers to. A step whose frame was evicted falls back to the
/// wireframe.
public final class ViewerFrameStore: Sendable {
    private struct Entry {
        var key: ViewerFrameKey
        var frame: CapturedScreenshot
    }

    private struct State {
        /// Oldest first.
        var entries: [Entry] = []
        var bytes = 0
        var enabled = true
        var shown = false
    }

    /// At most this many frames are kept (at least 1).
    public let maxFrames: Int
    /// At most this many encoded bytes are kept (at least 1). A single frame larger than
    /// this is not kept at all.
    public let maxBytes: Int
    private let state = Mutex(State())

    public init(maxFrames: Int = 24, maxBytes: Int = 24 * 1_024 * 1_024) {
        self.maxFrames = max(1, maxFrames)
        self.maxBytes = max(1, maxBytes)
    }

    /// Whether frames are accepted. Turning it off drops every frame at once and refuses
    /// new ones (the owner turned the option off while a run was going).
    public var isEnabled: Bool {
        get { state.withLock { $0.enabled } }
        set {
            state.withLock { s in
                s.enabled = newValue
                if !newValue {
                    s.entries.removeAll()
                    s.bytes = 0
                }
            }
        }
    }

    /// Keeps `frame` under `key` (replacing a frame already there) and evicts the oldest
    /// frames until both caps hold. Returns whether it was kept (never while disabled).
    @discardableResult
    public func put(_ frame: CapturedScreenshot, for key: ViewerFrameKey) -> Bool {
        let size = frame.data.count
        return state.withLock { s in
            guard s.enabled else { return false }
            if let i = s.entries.firstIndex(where: { $0.key == key }) {
                s.bytes -= s.entries[i].frame.data.count
                s.entries.remove(at: i)
            }
            guard size <= maxBytes else { return false }
            s.entries.append(Entry(key: key, frame: frame))
            s.bytes += size
            while s.entries.count > maxFrames || s.bytes > maxBytes {
                s.bytes -= s.entries.removeFirst().frame.data.count
            }
            return true
        }
    }

    /// The frame under `key`, `nil` when none was kept or it was evicted.
    public func frame(for key: ViewerFrameKey) -> CapturedScreenshot? {
        state.withLock { s in s.entries.first(where: { $0.key == key })?.frame }
    }

    /// The keys held, oldest first.
    public var keys: [ViewerFrameKey] { state.withLock { $0.entries.map(\.key) } }

    /// Frames held.
    public var count: Int { state.withLock { $0.entries.count } }

    /// Encoded bytes held.
    public var byteCount: Int { state.withLock { $0.bytes } }

    /// Whether a Live view showing these frames is on screen now (the run view is up and
    /// the app is active). The app sets it; nothing else depends on it.
    public var isShown: Bool {
        get { state.withLock { $0.shown } }
        set { state.withLock { $0.shown = newValue } }
    }

    /// A run ended: when no Live view is on screen (a run started remotely or by a schedule
    /// that nobody watches), its real-screen frames go now. The wireframe timeline is not
    /// in here and stays. Returns whether the frames were dropped.
    @discardableResult
    public func runEnded() -> Bool {
        state.withLock { s in
            guard !s.shown else { return false }
            s.entries.removeAll()
            s.bytes = 0
            return true
        }
    }

    /// Drops every frame (a new run starts, or the owner turned the option off).
    public func removeAll() {
        state.withLock { s in
            s.entries.removeAll()
            s.bytes = 0
        }
    }
}
