import Foundation
import JarvisCore
import Synchronization

/// The picture of the approval question on screen ("Show the real screen when Jarvis asks"):
/// **one slot, memory only**, never written to disk, never logged, never streamed, never in
/// the timeline, the delta stream or the history, never sent to a paired device. Separate
/// from `ViewerFrameStore` (the Live view's), so its lifetime is the question's alone.
///
/// Capture, show and hold are three rules (preview review):
/// - **Capture** whenever the phone is known unlocked and the session is not waiting for an
///   unlock, whichever app is in front: when Jarvis asks, the target app is in front, and
///   that app's screen is what the picture is of. `isPhoneUnlocked` (protected data
///   available) and the owner's option (`isEnabled`) gate it here; the driver checks the
///   runner's lock state.
/// - **Show** only while Jarvis is active with the sheet up (`ApprovalPreview.make`'s
///   `appActive`, and the sheet itself).
/// - **Hold** until the question is resolved (approved, declined, timed out, withdrawn, a
///   lock, the run cancelled or ended: the driver and the coordinator remove it) or the
///   phone locks / protected data becomes unavailable (`isPhoneUnlocked = false`). Not
///   dropped because Jarvis is in the background: that is where the owner is while on the
///   way to the sheet.
///
/// The store also names the question that is open (`questionKey`, no pixels): the driver
/// sets it for exactly as long as a confirmation is asked, so the sheet never shows a
/// picture computed for another question with the same text (`ApprovalPreview.make`).
public final class ApprovalFrameStore: Sendable {
    private struct State {
        var key: ViewerFrameKey?
        var frame: CapturedScreenshot?
        /// Bumped by every frame kept: names the frame for an image cache.
        var sequence: UInt64 = 0
        var question: ViewerFrameKey?
        var enabled = true
        var phoneUnlocked = true
    }

    /// A single frame larger than this is not kept (the Live view store's cap style).
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

    /// The phone is unlocked (protected data available). Turning it off (the phone is
    /// locking) drops the frame at once and refuses new ones until it is on again. Whether
    /// Jarvis is in front does not matter here (see the type's rules).
    public var isPhoneUnlocked: Bool {
        get { state.withLock { $0.phoneUnlocked } }
        set {
            state.withLock { s in
                s.phoneUnlocked = newValue
                if !newValue { s.key = nil; s.frame = nil }
            }
        }
    }

    /// Whether a frame would be kept now (the driver asks before it captures anything).
    public var accepts: Bool { state.withLock { $0.enabled && $0.phoneUnlocked } }

    /// Keeps `frame` as the frame of observation `key`, replacing any other. Returns
    /// whether it was kept.
    @discardableResult
    public func put(_ frame: CapturedScreenshot, for key: ViewerFrameKey) -> Bool {
        state.withLock { s in
            guard s.enabled, s.phoneUnlocked, frame.data.count <= maxBytes else { return false }
            s.key = key
            s.frame = frame
            s.sequence &+= 1
            return true
        }
    }

    /// The frame of observation `key`; `nil` for any other key.
    public func frame(for key: ViewerFrameKey) -> CapturedScreenshot? {
        state.withLock { s in s.key == key ? s.frame : nil }
    }

    /// The frame of observation `key` with its sequence number (a new number for every frame
    /// kept, so a cache never mistakes one frame for another under the same key).
    public func held(for key: ViewerFrameKey) -> (frame: CapturedScreenshot, sequence: UInt64)? {
        state.withLock { s in
            guard s.key == key, let frame = s.frame else { return nil }
            return (frame, s.sequence)
        }
    }

    /// The key of the frame held, if any.
    public var key: ViewerFrameKey? { state.withLock { $0.key } }

    /// Frames held (0 or 1).
    public var count: Int { state.withLock { $0.frame == nil ? 0 : 1 } }

    /// The observation the confirmation asked now was computed from (set by the driver for
    /// exactly as long as it asks; no pixels). `nil` while no confirmation is asked.
    public var questionKey: ViewerFrameKey? { state.withLock { $0.question } }

    /// A confirmation computed from observation `key` is asked now (kept whether or not the
    /// option is on: it holds no pixels).
    public func beginQuestion(_ key: ViewerFrameKey) {
        state.withLock { $0.question = key }
    }

    /// The confirmation computed from `key` is resolved: the question and its frame go.
    public func endQuestion(_ key: ViewerFrameKey) {
        state.withLock { s in
            if s.question == key { s.question = nil }
            if s.key == key { s.key = nil; s.frame = nil }
        }
    }

    /// Drops the frame of `key` (and only that one).
    public func remove(_ key: ViewerFrameKey) {
        state.withLock { s in
            if s.key == key { s.key = nil; s.frame = nil }
        }
    }

    /// Drops whatever is held, and the question.
    public func removeAll() {
        state.withLock { s in
            s.key = nil
            s.frame = nil
            s.question = nil
        }
    }
}

/// What the approval sheet shows beside the core's question: a picture of the screen from
/// when Jarvis asked, with the control the action names outlined. Built from the core's
/// viewer (`JarvisViewer.approvalTarget()`: the evaluated observation's key and screen, the
/// target in unit coordinates, the caption) and the approval frame store; the SwiftUI view
/// only draws it.
///
/// What is checked, and what is not: the outline is computed from the same tree and indices
/// the core used; pixels are shown only when they are the observation's own screenshot or a
/// tree read after them equals the evaluated tree (`SessionDriver`); the core re-checks the
/// tree digest before it acts, never pixels. Content outside the accessibility tree (images,
/// canvas, video) can differ or change. The picture is a display aid, not proof.
///
/// - **Bound to the question**: made only for a pending confirmation whose request is the
///   one the core's picture belongs to (`promptId`), and, while the driver names the open
///   question (`ApprovalFrameStore.questionKey`), only for a picture of that very
///   observation: a stale picture with the same request text is never shown.
/// - **Bound to the observation**: the screenshot is shown only when the store holds a
///   frame under exactly the evaluated observation's key and its pixel aspect ratio matches
///   the screen's (within 2%); otherwise the backdrop is the wireframe of that screen, with
///   the target outlined the same way.
/// - **Lifetime**: the question's. The driver drops the frame when the question is
///   resolved; a preview whose prompt is no longer pending is not shown (`isLive`).
public struct ApprovalPreview: Sendable, Equatable {
    /// What is drawn behind the outline.
    public enum Backdrop: Sendable, Equatable {
        /// The screenshot of the evaluated observation ("Screen when Jarvis asked").
        case realScreen
        /// The wireframe of the evaluated observation's tree (or nothing, when there is none).
        case layout
    }

    /// The badges (preview review: they say what the picture is, never more).
    public static let screenBadge = "Screen when Jarvis asked"
    public static let layoutBadge = "Layout from the app's data"
    public static let textLayoutBadge = "Layout from the runner's text"
    public static let noPictureBadge = "No picture of this screen"

    /// Largest relative difference between the frame's pixel aspect ratio and the screen's
    /// for the frame to be drawn under the outline.
    public static let maxAspectMismatch = 0.02

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

    /// The small label on the picture, for a layout drawn from the app's data.
    public var badge: String {
        backdrop == .realScreen ? Self.screenBadge : Self.layoutBadge
    }

    /// The small label on the picture drawn with `screen` (the evaluated screen's
    /// wireframe, `nil` when it is not available): the screenshot, else the layout and where
    /// it came from, else that there is no picture.
    public func badge(screen: CoreViewerScreen?) -> String {
        badge(screen: screen, drawsScreenshot: backdrop == .realScreen)
    }

    /// The badge for what a view actually draws: `drawsScreenshot` only when it drew the
    /// decoded screenshot (a frame that could not be decoded, or is not drawn while the app
    /// is not active, is not claimed).
    public func badge(screen: CoreViewerScreen?, drawsScreenshot: Bool) -> String {
        if drawsScreenshot && backdrop == .realScreen { return Self.screenBadge }
        guard let screen, !screen.elements.isEmpty else { return Self.noPictureBadge }
        return screen.info.fromText ? Self.textLayoutBadge : Self.layoutBadge
    }

    /// Whether `frame` may be drawn as the screen of `size`: both known and their aspect
    /// ratios within `maxAspectMismatch` (a sideways or cropped capture would put the
    /// outline on another part of the picture). Pixel sizes the runner did not report (0)
    /// fail.
    public static func aspectMatches(_ frame: CapturedScreenshot, screen size: Size) -> Bool {
        let w = size.width, h = size.height
        guard frame.pixelWidth > 0, frame.pixelHeight > 0, w.isFinite, h.isFinite, w > 0, h > 0 else { return false }
        let frameAspect = Double(frame.pixelWidth) / Double(frame.pixelHeight)
        let screenAspect = w / h
        return abs(frameAspect / screenAspect - 1) <= maxAspectMismatch
    }

    /// Width / height of the screen (an iPhone in portrait when the size is unusable),
    /// clamped to 0.3…3.
    public var aspectRatio: Double {
        let w = screenSize.width, h = screenSize.height
        guard w.isFinite, h.isFinite, w > 0, h > 0 else { return 393.0 / 852.0 }
        return min(3, max(0.3, w / h))
    }

    /// The preview of `prompt`, when it is a pending confirmation and the core's picture
    /// (`target`) belongs to that very request and, while the store names the open question,
    /// to that question's observation. `realScreen` only when `frames` holds a frame under
    /// exactly the evaluated observation's key whose aspect ratio matches the screen's, and
    /// `appActive` (the screenshot is never drawn while the app is not in front).
    public static func make(
        prompt: HumanPrompt?,
        target: CoreApprovalTarget?,
        frames: ApprovalFrameStore?,
        appActive: Bool
    ) -> ApprovalPreview? {
        guard case .confirm(let id, let request)? = prompt, let target, target.request == request,
              let key = ViewerFrameKey(target.frameKey) else { return nil }
        // A picture computed for an earlier question with the same text (a stale target)
        // is not this question's.
        if let question = frames?.questionKey, question != key { return nil }
        let frame = frames?.frame(for: key)
        let real = appActive && frame.map { aspectMatches($0, screen: target.screenSize) } == true
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
        held(in: frames)?.frame
    }

    /// The real frame to draw with its store sequence number (an image cache keys on it).
    public func held(in frames: ApprovalFrameStore) -> (frame: CapturedScreenshot, sequence: UInt64)? {
        guard backdrop == .realScreen, let held = frames.held(for: key),
              Self.aspectMatches(held.frame, screen: screenSize) else { return nil }
        return held
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
