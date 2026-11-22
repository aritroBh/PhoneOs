import Foundation
import JarvisCore
import Synchronization

/// The Live view of one run: a thin shell around the core's `JarvisViewer`, which holds all
/// of its logic (the timeline reducer, the privacy rules, the wire format, the cursor path,
/// replay and the scene; see `core/jarvis-viewer`). This type only crosses the FFI:
///
/// - **One call per update**: `apply(_:)` maps a `RunEvent` to a `CoreViewerUpdate` in one
///   switch and calls `feed` once. The light timeline (steps and screen summaries, no
///   wireframes) is fetched again only when the change says something changed.
/// - **One call per drawn frame**: `scene(_:)`.
/// - **On a change of the pending confirmation**: `approvalTarget()` (the approval sheet's
///   picture; see `ApprovalPreview`).
/// - **On demand**: `screen(id:)` (a wireframe, cached by id: a screen never changes once
///   made; the cache is bounded and drops screens the timeline no longer refers to),
///   `replay(_:nowMs:reduceMotion:)`, `replayStatus()`, `encode()`.
///
/// Every FFI error is absorbed: the view shows nothing (or the last good timeline) rather
/// than crashing, and the next update is fed as usual.
public final class LiveViewModel: Sendable {
    /// Steps kept (the core clamps to 1…200).
    public static let maxSteps: UInt32 = 200
    /// Wireframes kept in the cache.
    public static let maxCachedScreens = 4
    /// Apps named by their display name in "Open …" captions.
    public static var knownApps: [AppInfo] { KnownApps.common + DemoPhone.knownApps }

    private struct State {
        var timeline: CoreViewerTimeline = .empty
        /// A feed or a timeline fetch failed: the next successful feed fetches the timeline.
        var stale = false
        /// Most recently used last.
        var screens: [CoreViewerScreen] = []
        /// The core's picture of the confirmation pending now (fetched when it changes).
        var approval: CoreApprovalTarget?
    }

    private let viewer: (any JarvisViewerProtocol)?
    private let state = Mutex(State())

    /// An empty Live view (`framesEnabled`: the driver keeps real-screen frames for this run,
    /// so unlocked screens name a frame key).
    public convenience init(framesEnabled: Bool = false) {
        let config = CoreViewerConfig(maxSteps: Self.maxSteps, framesEnabled: framesEnabled, knownApps: Self.knownApps)
        self.init(viewer: try? JarvisViewer(config: config))
    }

    /// Wraps `viewer` (tests pass a counting or failing one); `nil` shows nothing.
    init(viewer: (any JarvisViewerProtocol)?) {
        self.viewer = viewer
        if let timeline = try? viewer?.timeline() {
            state.withLock { $0.timeline = timeline }
        }
    }

    // MARK: - Updates

    /// The core's form of `event` (one switch, no logic: the core decides what matters).
    static func update(for event: RunEvent) -> CoreViewerUpdate {
        switch event {
        case .prompt(let text): .prompt(text: text)
        case .reply: .ignored
        case .update(let update):
            switch update {
            case .started(let spec): .started(taskId: spec.id)
            case .effect(let effect, let number): .effect(effect: effect, number: UInt64(clamping: number))
            case .progress(let progress): .progress(progress: progress)
            case .confirmationPending(let request): .confirmationPending(request: request)
            case .questionPending: .questionPending
            case .awaitingUnlock: .awaitingUnlock
            case .effectCompleted(let effect, let event): .effectCompleted(effect: effect, event: event)
            case .finished(let result): .finished(status: result.outcome.status)
            case .lockDetected, .unlockWaitEnded, .ioFailed, .note: .ignored
            }
        }
    }

    /// Feeds one event (one FFI call). Returns whether `timeline` or `approvalTarget`
    /// changed (a failed feed drops the approval picture: whatever the core holds now is
    /// unknown).
    @discardableResult
    public func apply(_ event: RunEvent) -> Bool {
        guard let viewer else { return false }
        let update = Self.update(for: event)
        return state.withLock { s in
            let change: CoreViewerChange
            do {
                change = try viewer.feed(update: update)
            } catch {
                s.stale = true
                // The core may have moved on (a newer question, or none): a picture of an
                // earlier question is not kept for whatever is asked next (preview review).
                let hadApproval = s.approval != nil
                s.approval = nil
                return hadApproval
            }
            if change.approval {
                s.approval = (try? viewer.approvalTarget()) ?? nil
            }
            guard change.steps || change.screens || change.activity || change.approval || s.stale else { return false }
            do {
                s.timeline = try viewer.timeline()
                s.stale = false
            } catch {
                s.stale = true
                return false
            }
            let kept = Set(s.timeline.screens.map(\.id))
            s.screens.removeAll { !kept.contains($0.info.id) }
            return true
        }
    }

    /// Steps, screen summaries and the activity (republished by `apply` only on a change).
    public var timeline: CoreViewerTimeline { state.withLock { $0.timeline } }

    /// The core's picture of the confirmation pending now (the evaluated observation's key
    /// and screen, the target, the caption), for the approval sheet; `nil` when none is
    /// pending. Fetched once per change (one FFI call), never encoded.
    public var approvalTarget: CoreApprovalTarget? { state.withLock { $0.approval } }

    // MARK: - Drawing

    /// The screen with `id` and its wireframe: from the cache, else one FFI call. `nil`
    /// when the timeline no longer refers to it, or the call failed.
    public func screen(id: UInt32?) -> CoreViewerScreen? {
        guard let id, let viewer else { return nil }
        return state.withLock { s in
            if let i = s.screens.firstIndex(where: { $0.info.id == id }) {
                let screen = s.screens.remove(at: i)
                s.screens.append(screen)
                return screen
            }
            guard let screen = try? viewer.screen(id: id) else { return nil }
            s.screens.append(screen)
            if s.screens.count > Self.maxCachedScreens { s.screens.removeFirst(s.screens.count - Self.maxCachedScreens) }
            return screen
        }
    }

    /// Wireframes held in the cache.
    public var cachedScreenCount: Int { state.withLock { $0.screens.count } }

    /// What to draw now (one FFI call; it also syncs the scrubber and advances playback).
    /// `nil` when the call failed: draw nothing.
    public func scene(_ query: CoreSceneQuery) -> CoreLiveScene? {
        try? viewer?.scene(query: query)
    }

    /// Whether the view must keep redrawing every display frame at `nowMs` (one scene
    /// query; `false` when it failed, so a broken viewer never keeps the display busy).
    public func needsFrames(nowMs: Double, reduceMotion: Bool) -> Bool {
        scene(CoreSceneQuery(nowMs: nowMs, reduceMotion: reduceMotion, viewWidth: 0, viewHeight: 0))?.needsFrames ?? false
    }

    /// The real-screen frame `screen` may have in `ViewerFrameStore`.
    public func frameKey(for screen: CoreViewerScreenInfo?) -> ViewerFrameKey? {
        screen?.frameKey.flatMap(ViewerFrameKey.init)
    }

    // MARK: - Replay

    /// A scrubber control at `nowMs`; the scrubber's state with its texts, `nil` when the
    /// call failed.
    @discardableResult
    public func replay(_ action: CoreReplayAction, nowMs: Double, reduceMotion: Bool) -> CoreReplayStatus? {
        try? viewer?.replay(action: action, nowMs: nowMs, reduceMotion: reduceMotion)
    }

    /// The scrubber's state with its texts.
    public func replayStatus() -> CoreReplayStatus? {
        try? viewer?.replayStatus()
    }

    /// The timeline's wire form (for the remote stream); `nil` when the call failed.
    public func encode() -> Data? {
        try? viewer?.encode()
    }
}
