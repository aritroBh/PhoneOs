import Foundation
import JarvisCore
import Synchronization

/// The real-screen frame of the approval question on screen ("Show the real screen when
/// Jarvis asks"): **one slot, memory only**, never written to disk, never logged, never
/// streamed, never in the timeline or the history. Separate from `ViewerFrameStore` (the
/// Live view's), so its lifetime is the question's alone.
///
/// `SessionDriver` puts a frame here just before it asks the owner and removes it as soon
/// as the question is resolved, whatever resolved it (approved, declined, timed out,
/// withdrawn, a lock, the run cancelled). The coordinator empties it when a run starts and
/// ends; the app empties it when it leaves the foreground or the phone locks
/// (`isAppActive = false`).
public final class ApprovalFrameStore: Sendable {
    private struct State {
        var key: ViewerFrameKey?
        var frame: CapturedScreenshot?
        var enabled = true
        var appActive = true
    }

    /// A single frame larger than this is not kept.
    public let maxBytes: Int
    private let state = Mutex(State())

    public init(maxBytes: Int = 8 * 1_024 * 1_024) {
        self.maxBytes = max(1, maxBytes)
    }

    /// The owner's option. Turning it off drops the frame at once and refuses new ones.
    public var isEnabled: Bool {
        get { state.withLock { $0.enabled } }
        set {
            state.withLock { s in
                s.enabled = newValue
                if !newValue { s.key = nil; s.frame = nil }
            }
        }
    }

    /// The app is in the foreground with the phone unlocked. Turning it off drops the frame
    /// at once and refuses new ones until it is on again.
    public var isAppActive: Bool {
        get { state.withLock { $0.appActive } }
        set {
            state.withLock { s in
                s.appActive = newValue
                if !newValue { s.key = nil; s.frame = nil }
            }
        }
    }

    /// Whether a frame would be kept now (the driver asks before it captures anything).
    public var accepts: Bool { state.withLock { $0.enabled && $0.appActive } }

    /// Keeps `frame` as the frame of observation `key`, replacing any other. Returns
    /// whether it was kept.
    @discardableResult
    public func put(_ frame: CapturedScreenshot, for key: ViewerFrameKey) -> Bool {
        state.withLock { s in
            guard s.enabled, s.appActive, frame.data.count <= maxBytes else { return false }
            s.key = key
            s.frame = frame
            return true
        }
    }

    /// The frame of observation `key`; `nil` for any other key.
    public func frame(for key: ViewerFrameKey) -> CapturedScreenshot? {
        state.withLock { s in s.key == key ? s.frame : nil }
    }

    /// The key of the frame held, if any.
    public var key: ViewerFrameKey? { state.withLock { $0.key } }

    /// Frames held (0 or 1).
    public var count: Int { state.withLock { $0.frame == nil ? 0 : 1 } }

    /// Drops the frame of `key` (and only that one).
    public func remove(_ key: ViewerFrameKey) {
        state.withLock { s in
            if s.key == key { s.key = nil; s.frame = nil }
        }
    }

    /// Drops whatever is held.
    public func removeAll() {
        state.withLock { s in
            s.key = nil
            s.frame = nil
        }
    }
}

/// What the approval sheet shows beside the core's question: the screen Jarvis evaluated,
/// with the control it will act on outlined. Built from the core's viewer
/// (`JarvisViewer.approvalTarget()`: the evaluated observation's key and screen, the target
/// in unit coordinates, the caption) and the approval frame store; the SwiftUI view only
/// draws it.
///
/// - **Bound to the question**: made only for a pending confirmation whose request is the
///   one the core's picture belongs to (`promptId`).
/// - **Bound to the observation**: the real screen is shown only when the store holds a
///   frame under exactly the evaluated observation's key; otherwise the backdrop is the
///   wireframe of that screen ("Layout only"), with the target outlined the same way.
/// - **Lifetime**: the question's. The driver drops the frame when the question is
///   resolved; a preview whose prompt is no longer pending is not shown (`isLive`).
public struct ApprovalPreview: Sendable, Equatable {
    /// What is drawn behind the outline.
    public enum Backdrop: Sendable, Equatable {
        /// The real screen of the evaluated observation.
        case realScreen
        /// The wireframe of the evaluated observation's tree.
        case layout
    }

    /// What is marked, in unit coordinates (0…1 of the screen's width and height).
    public enum Mark: Sendable, Equatable {
        case outline(Rect)
        case point(Point)
        case none
    }

    /// The confirmation prompt this preview belongs to.
    public var promptId: Int
    /// The evaluated observation.
    public var key: ViewerFrameKey
    /// Its screen in the Live view (`LiveViewModel.screen(id:)` gives the wireframe).
    public var screenId: UInt32
    /// Its size in points.
    public var screenSize: Size
    public var backdrop: Backdrop
    public var mark: Mark
    /// One short line under the picture.
    public var caption: String
    /// For VoiceOver ("Outlined: Button 'Pay $512.40'").
    public var accessibilityLabel: String

    /// The small label on the picture.
    public var badge: String {
        backdrop == .realScreen ? "What Jarvis sees now" : "Layout only"
    }

    /// Width / height of the screen (an iPhone in portrait when the size is unusable),
    /// clamped to 0.3…3.
    public var aspectRatio: Double {
        let w = screenSize.width, h = screenSize.height
        guard w.isFinite, h.isFinite, w > 0, h > 0 else { return 393.0 / 852.0 }
        return min(3, max(0.3, w / h))
    }

    /// The preview of `prompt`, when it is a pending confirmation and the core's picture
    /// (`target`) belongs to that very request. `realScreen` only when `frames` holds a
    /// frame under exactly the evaluated observation's key and `appActive` (the real screen
    /// is never drawn while the app is not in front).
    public static func make(
        prompt: HumanPrompt?,
        target: CoreApprovalTarget?,
        frames: ApprovalFrameStore?,
        appActive: Bool
    ) -> ApprovalPreview? {
        guard case .confirm(let id, let request)? = prompt, let target, target.request == request,
              let key = ViewerFrameKey(target.frameKey) else { return nil }
        let real = appActive && frames?.frame(for: key) != nil
        let mark: Mark
        switch target.mark {
        case .outline(_, let unit): mark = .outline(unit)
        case .point(_, let unit): mark = .point(unit)
        case .nothing: mark = .none
        }
        return ApprovalPreview(
            promptId: id, key: key, screenId: target.screenId, screenSize: target.screenSize,
            backdrop: real ? .realScreen : .layout, mark: mark, caption: target.caption,
            accessibilityLabel: target.accessibilityLabel
        )
    }

    /// Whether this preview may still be shown: its prompt is the one pending.
    public func isLive(pending: HumanPrompt?) -> Bool {
        guard case .confirm(let id, _)? = pending else { return false }
        return id == promptId
    }

    /// The real frame to draw, when `backdrop` is `realScreen` and the store still holds it.
    public func frame(in frames: ApprovalFrameStore) -> CapturedScreenshot? {
        backdrop == .realScreen ? frames.frame(for: key) : nil
    }

    /// `rect` in unit coordinates placed in a view of `width` × `height` points (the
    /// picture is drawn at the screen's aspect ratio, so unit coordinates map directly);
    /// `nil` for a rectangle with nothing on the screen.
    public static func place(_ rect: Rect, width: Double, height: Double) -> Rect? {
        let values = [rect.x, rect.y, rect.width, rect.height, width, height]
        guard values.allSatisfy(\.isFinite), rect.width > 0, rect.height > 0, width > 0, height > 0 else { return nil }
        let left = max(rect.x, 0), top = max(rect.y, 0)
        let right = min(rect.x + rect.width, 1), bottom = min(rect.y + rect.height, 1)
        guard right > left, bottom > top else { return nil }
        return Rect(x: left * width, y: top * height, width: (right - left) * width, height: (bottom - top) * height)
    }

    /// A wireframe element's frame (screen points) in unit coordinates of this screen.
    public func unit(_ frame: Rect) -> Rect? {
        let w = screenSize.width, h = screenSize.height
        guard w.isFinite, h.isFinite, w > 0, h > 0 else { return nil }
        return Rect(x: frame.x / w, y: frame.y / h, width: frame.width / w, height: frame.height / h)
    }
}
