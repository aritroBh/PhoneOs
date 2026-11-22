import Foundation

/// Where the Live view's cursor is at a moment of a step: the one source of truth for the
/// animation (the SwiftUI view only asks for `pose(…)` every frame).
///
/// A pointer step glides from where the cursor rested to the press point (ease-in-out, the
/// duration grows with the distance and is clamped), then presses (tap ripple, long-press
/// ring filling over the hold) or drags along the swipe, and rests on the end point. Other
/// gestures (typing, keys, opening an app or URL, waiting) leave the cursor parked where it
/// was. All points are clamped onto the screen; NaN and negative times are handled.
public enum CursorPath {
    /// Glide durations: `glideMsPerPoint` per point of distance, clamped to this range.
    public static let glideRangeMs: ClosedRange<Double> = 180...700
    public static let glideMsPerPoint: Double = 1.1
    /// A tap's press (the ripple).
    public static let tapMs: Double = 280
    /// A long press shows its real hold, clamped to this range.
    public static let longPressRangeMs: ClosedRange<Double> = 300...2_500
    /// A swipe shows its real duration, clamped to this range.
    public static let swipeRangeMs: ClosedRange<Double> = 200...1_500
    /// How long a non-pointer step plays (its caption is the show).
    public static let parkedMs: Double = 700
    /// A wait plays its real length, clamped to this range.
    public static let waitRangeMs: ClosedRange<Double> = 400...1_500
    /// Pause after a step before replay moves to the next, so the result can be seen.
    public static let settleMs: Double = 450

    public enum Phase: String, Sendable, Equatable {
        /// Moving to the press point.
        case gliding
        /// Finger down on the point (tap ripple / long-press ring); `progress` runs 0…1.
        case pressing
        /// Dragging from the press point to the end point; `progress` runs 0…1.
        case swiping
        /// Done; resting on the end point.
        case resting
        /// A non-pointer step: the cursor stays where it was.
        case parked
    }

    public struct Pose: Sendable, Equatable {
        /// In screen points, on the screen.
        public var position: ViewerPoint
        public var phase: Phase
        /// Progress through `phase`, 0…1.
        public var progress: Double

        /// The finger is down.
        public var isPressed: Bool { phase == .pressing || phase == .swiping }

        public init(position: ViewerPoint, phase: Phase, progress: Double) {
            self.position = position
            self.phase = phase
            self.progress = progress
        }
    }

    public struct Timing: Sendable, Equatable {
        public var glideMs: Double
        public var actionMs: Double
        public var totalMs: Double { glideMs + actionMs }
    }

    /// Ease-in-out (smoothstep): 0 → 0, 1 → 1, monotone, flat at both ends. Clamped.
    public static func ease(_ t: Double) -> Double {
        guard t.isFinite else { return 0 }
        let x = min(max(t, 0), 1)
        return x * x * (3 - 2 * x)
    }

    /// Glide duration for a distance in points.
    public static func glideMs(distance: Double) -> Double {
        let d = distance.isFinite ? max(distance, 0) : 0
        return min(max(d * glideMsPerPoint, glideRangeMs.lowerBound), glideRangeMs.upperBound)
    }

    static func clamped(_ value: Double, _ range: ClosedRange<Double>) -> Double {
        min(max(value.isFinite ? value : range.lowerBound, range.lowerBound), range.upperBound)
    }

    /// How long `gesture` plays when the cursor starts at `rest`. Reduce Motion: no glide.
    public static func timing(_ gesture: ViewerGesture, from rest: ViewerPoint, screen: ViewerSize,
                              reduceMotion: Bool = false) -> Timing {
        let start = screen.clamp(rest)
        switch gesture {
        case .tap(let p):
            return Timing(glideMs: reduceMotion ? 0 : glideMs(distance: start.distance(to: screen.clamp(p))), actionMs: tapMs)
        case .longPress(let p, let ms):
            return Timing(glideMs: reduceMotion ? 0 : glideMs(distance: start.distance(to: screen.clamp(p))),
                          actionMs: clamped(Double(ms), longPressRangeMs))
        case .swipe(let from, _, let ms):
            return Timing(glideMs: reduceMotion ? 0 : glideMs(distance: start.distance(to: screen.clamp(from))),
                          actionMs: clamped(Double(ms), swipeRangeMs))
        case .wait(let ms):
            return Timing(glideMs: 0, actionMs: clamped(Double(ms), waitRangeMs))
        case .type, .key, .launch, .openURL:
            return Timing(glideMs: 0, actionMs: parkedMs)
        }
    }

    /// How long replay shows a step before moving on (its timing plus `settleMs`).
    public static func playMs(_ gesture: ViewerGesture, from rest: ViewerPoint, screen: ViewerSize,
                              reduceMotion: Bool = false) -> Double {
        timing(gesture, from: rest, screen: screen, reduceMotion: reduceMotion).totalMs + settleMs
    }

    /// The cursor `t` milliseconds into `gesture`, starting from `rest`.
    public static func pose(_ gesture: ViewerGesture, from rest: ViewerPoint, screen: ViewerSize, atMs t: Double,
                            reduceMotion: Bool = false) -> Pose {
        let time = t.isFinite ? max(t, 0) : 0
        let start = screen.clamp(rest)
        guard let pressRaw = gesture.pressPoint, let endRaw = gesture.endPoint else {
            let timing = timing(gesture, from: rest, screen: screen, reduceMotion: reduceMotion)
            return Pose(position: start, phase: .parked, progress: fraction(time, timing.actionMs))
        }
        let press = screen.clamp(pressRaw)
        let end = screen.clamp(endRaw)
        let timing = timing(gesture, from: rest, screen: screen, reduceMotion: reduceMotion)
        if time < timing.glideMs {
            return Pose(position: start.interpolated(to: press, ease(time / timing.glideMs)), phase: .gliding,
                        progress: time / timing.glideMs)
        }
        let inAction = time - timing.glideMs
        if inAction < timing.actionMs {
            let p = fraction(inAction, timing.actionMs)
            if case .swipe = gesture {
                // Reduce Motion: the cursor does not travel; the trail still shows the drag.
                let position = reduceMotion ? end : press.interpolated(to: end, ease(p))
                return Pose(position: position, phase: .swiping, progress: p)
            }
            return Pose(position: press, phase: .pressing, progress: p)
        }
        return Pose(position: end, phase: .resting, progress: 1)
    }

    static func fraction(_ t: Double, _ total: Double) -> Double {
        guard total > 0, t.isFinite else { return 1 }
        return min(max(t / total, 0), 1)
    }

    /// Where the cursor rests after `gesture` started from `rest`.
    public static func restingPoint(after gesture: ViewerGesture, from rest: ViewerPoint, screen: ViewerSize) -> ViewerPoint {
        guard let end = gesture.endPoint else { return screen.clamp(rest) }
        return screen.clamp(end)
    }

    /// Where the cursor rests before step `number` of `timeline`: the end of the latest
    /// pointer step before it, else the centre of the step's screen (the first step glides
    /// in from the middle).
    public static func restingPoint(before number: Int, in timeline: ViewerTimeline) -> ViewerPoint {
        guard let index = timeline.steps.firstIndex(where: { $0.number == number }) else {
            return timeline.currentScreen?.size.center ?? ViewerSize.fallbackPhone.center
        }
        let size = timeline.screen(for: timeline.steps[index])?.size ?? ViewerSize.fallbackPhone
        var i = index - 1
        while i >= 0 {
            let step = timeline.steps[i]
            if let end = step.gesture.endPoint {
                let stepSize = timeline.screen(for: step)?.size ?? size
                return stepSize.clamp(end)
            }
            i -= 1
        }
        return size.center
    }

    /// Where the cursor rests after the newest step (or the centre of the current screen).
    public static func restingPoint(afterLatestOf timeline: ViewerTimeline) -> ViewerPoint {
        let size = timeline.currentScreen?.size ?? ViewerSize.fallbackPhone
        for step in timeline.steps.reversed() {
            if let end = step.gesture.endPoint {
                return (timeline.screen(for: step)?.size ?? size).clamp(end)
            }
        }
        return size.center
    }

    /// The pose of step `number` of `timeline` at `t` ms (the view's one call per frame).
    public static func pose(step number: Int, in timeline: ViewerTimeline, atMs t: Double, reduceMotion: Bool = false) -> Pose? {
        guard let step = timeline.step(number: number) else { return nil }
        let size = timeline.screen(for: step)?.size ?? ViewerSize.fallbackPhone
        let rest = restingPoint(before: number, in: timeline)
        return pose(step.gesture, from: rest, screen: size, atMs: t, reduceMotion: reduceMotion)
    }

    /// How long replay shows step `number` of `timeline`.
    public static func playMs(step number: Int, in timeline: ViewerTimeline, reduceMotion: Bool = false) -> Double {
        guard let step = timeline.step(number: number) else { return settleMs }
        let size = timeline.screen(for: step)?.size ?? ViewerSize.fallbackPhone
        return playMs(step.gesture, from: restingPoint(before: number, in: timeline), screen: size, reduceMotion: reduceMotion)
    }
}
