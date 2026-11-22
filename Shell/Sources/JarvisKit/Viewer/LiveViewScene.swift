import Foundation

/// Everything the Live view draws at one moment, decided here so the SwiftUI view has no
/// logic of its own: which screen is the backdrop, which step (if any) is playing, the
/// cursor's pose and the caption.
///
/// While following live, the newest step plays to its end even if Jarvis has already moved
/// on (the command ran before the viewer heard of it); after that the cursor idles on the
/// latest screen and the caption says what Jarvis is doing ("Thinking…", "Waiting for your
/// OK · Delete", "Done"). In replay the scrubbed step is shown on the screen it acted on.
public struct LiveViewScene: Sendable, Equatable {
    /// The backdrop's screen.
    public var screen: ViewerScreen?
    /// The step being shown (`nil` while idle with no step playing).
    public var step: ViewerStep?
    public var pose: CursorPath.Pose
    public var caption: String
    /// The cursor idles (subtle pulse): Jarvis thinks, looks or waits.
    public var isIdle: Bool
    /// The run is over.
    public var isFinished: Bool
    /// "Step 3 of 12" in replay, `nil` while following.
    public var position: String?

    /// The VoiceOver value of the viewer: the caption, with the position in replay.
    public var accessibilityValue: String {
        var parts = [caption]
        if let step, step.state.isFailed { parts.append("did not work") }
        if let step, step.state == .stopped { parts.append("stopped") }
        if let position { parts.append(position) }
        return parts.joined(separator: ", ")
    }

    public static func make(timeline: ViewerTimeline, replay: ReplayState, now: Double, reduceMotion: Bool = false) -> LiveViewScene {
        let finished: Bool = {
            if case .finished = timeline.activity { return true }
            return false
        }()
        let numbers = timeline.stepNumbers
        if !replay.isFollowing, let number = replay.current, let step = timeline.step(number: number) {
            let pose = CursorPath.pose(step: number, in: timeline, atMs: replay.elapsedMs(now: now), from: replay.origin,
                                       reduceMotion: reduceMotion)
            let index = (numbers.firstIndex(of: number) ?? 0) + 1
            return LiveViewScene(
                screen: timeline.screen(for: step), step: step,
                pose: pose ?? idlePose(timeline), caption: step.caption, isIdle: false, isFinished: finished,
                position: "Step \(index) of \(numbers.count)"
            )
        }
        // Following: the newest step plays out, then the cursor idles on the latest screen.
        if let step = timeline.steps.last {
            let shown = replay.current == step.number
            let elapsed = shown ? replay.elapsedMs(now: now) : .infinity
            let origin = shown ? replay.origin : nil
            let playing = timeline.activity == .acting
                || elapsed < CursorPath.playMs(step: step.number, in: timeline, from: origin, reduceMotion: reduceMotion)
            if playing {
                let t = elapsed.isFinite ? elapsed : 0
                let pose = CursorPath.pose(step: step.number, in: timeline, atMs: t, from: origin, reduceMotion: reduceMotion)
                return LiveViewScene(
                    screen: timeline.screen(for: step) ?? timeline.currentScreen, step: step,
                    pose: pose ?? idlePose(timeline), caption: step.caption, isIdle: false, isFinished: finished,
                    position: nil
                )
            }
        }
        let caption = timeline.activity == .acting ? (timeline.steps.last?.caption ?? timeline.activity.caption)
            : timeline.activity.caption
        return LiveViewScene(
            screen: timeline.currentScreen ?? timeline.steps.last.flatMap { timeline.screen(for: $0) },
            step: nil, pose: idlePose(timeline), caption: caption, isIdle: !finished, isFinished: finished, position: nil
        )
    }

    /// `replay` after the timeline changed (`ReplayState.sync`): while following, a new step
    /// starts from where the cursor is at `now` on the step shown so far, so a step that
    /// arrives mid-animation continues the cursor's path instead of making it jump.
    public static func synced(_ replay: ReplayState, timeline: ViewerTimeline, now: Double,
                              reduceMotion: Bool = false) -> ReplayState {
        var next = replay
        var cursor: ViewerPoint?
        if replay.isFollowing, let shown = replay.current, shown != timeline.steps.last?.number,
           let pose = CursorPath.pose(step: shown, in: timeline, atMs: replay.elapsedMs(now: now), from: replay.origin,
                                      reduceMotion: reduceMotion) {
            cursor = pose.position
        }
        next.sync(numbers: timeline.stepNumbers, now: now, cursor: cursor)
        return next
    }

    /// Whether the view must redraw every display frame at `now`: replay is playing, the
    /// shown step's animation is still in flight, or the cursor pulses while a run is going
    /// (not under Reduce Motion). Otherwise nothing on the screen moves and the view's
    /// `TimelineView` is paused: a finished run with replay paused is fully static once its
    /// shown step has played out.
    public static func needsFrames(timeline: ViewerTimeline, replay: ReplayState, now: Double,
                                   reduceMotion: Bool = false) -> Bool {
        if replay.isPlaying { return true }
        let finished: Bool = {
            if case .finished = timeline.activity { return true }
            return false
        }()
        let scene = make(timeline: timeline, replay: replay, now: now, reduceMotion: reduceMotion)
        if let step = scene.step, let shown = replay.current, shown == step.number {
            let elapsed = replay.elapsedMs(now: now)
            let total = CursorPath.playMs(step: shown, in: timeline, from: replay.origin, reduceMotion: reduceMotion)
            if elapsed < total { return true }
        }
        // The pulse of the idle cursor while a run is going.
        let running = !timeline.taskId.isEmpty && !finished
        return running && scene.isIdle && !reduceMotion
    }

    static func idlePose(_ timeline: ViewerTimeline) -> CursorPath.Pose {
        CursorPath.Pose(position: CursorPath.restingPoint(afterLatestOf: timeline), phase: .parked, progress: 1)
    }
}
