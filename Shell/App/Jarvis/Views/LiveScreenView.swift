import SwiftUI
import UIKit
import JarvisKit

/// The Live view: a phone-shaped viewer of the screen Jarvis acts on, with a cursor that
/// glides to each target and shows the tap, long press or swipe, a caption, and replay
/// controls (step back, play, step forward, scrubber, speed, Live).
///
/// Everything it shows is decided in JarvisKit: `LiveViewScene` (which screen, which step,
/// the caption), `CursorPath` (where the cursor is at each moment) and `ViewerMapping`
/// (screen points to view points). This view only draws, once per frame of
/// `TimelineView(.animation)`.
///
/// iOS does not let an app draw over other apps, so this viewer lives inside Jarvis.
struct LiveScreenView: View {
    var timeline: ViewerTimeline
    var frames: ViewerFrameStore

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var replay = ReplayState()
    @State private var images = FrameImageCache()

    var body: some View {
        VStack(spacing: Theme.gap) {
            HStack(alignment: .firstTextBaseline) {
                Text("Live view").font(Theme.sectionLabel).foregroundStyle(Theme.textSoft)
                Spacer()
                Text(timeline.currentScreen?.displayName ?? "")
                    .font(Theme.monoSmall)
                    .foregroundStyle(Theme.textTertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .padding(.horizontal, 6)

            TimelineView(.animation) { context in
                let now = context.date.timeIntervalSinceReferenceDate
                let scene = LiveViewScene.make(timeline: timeline, replay: replay, now: now, reduceMotion: reduceMotion)
                PhoneScreen(
                    scene: scene,
                    tone: tone(for: scene),
                    image: images.image(for: scene.screen?.frameKey, in: frames),
                    now: now,
                    reduceMotion: reduceMotion
                )
                .onChange(of: context.date) { _, date in
                    advancePlayback(now: date.timeIntervalSinceReferenceDate)
                }
            }
            .frame(maxWidth: .infinity)

            controls
        }
        .padding(EdgeInsets(top: 14, leading: 10, bottom: 10, trailing: 10))
        .card(fill: Theme.well)
        .onAppear { replay.sync(numbers: timeline.stepNumbers, now: Self.now()) }
        .onChange(of: timeline.stepNumbers) { _, numbers in
            replay.sync(numbers: numbers, now: Self.now())
        }
        .onChange(of: timeline.taskId) { _, _ in
            // A new run: follow it live, whatever the last replay was doing.
            var fresh = ReplayState()
            fresh.sync(numbers: timeline.stepNumbers, now: Self.now())
            replay = fresh
        }
    }

    private static func now() -> Double { Date().timeIntervalSinceReferenceDate }

    /// While playing: on to the next step once the shown one has played out.
    private func advancePlayback(now: Double) {
        guard replay.isPlaying else { return }
        let current = timeline
        let reduce = reduceMotion
        var next = replay
        next.advance(numbers: current.stepNumbers, now: now) { number in
            CursorPath.playMs(step: number, in: current, reduceMotion: reduce)
        }
        if next != replay { replay = next }
    }

    /// The caption dot's colour: acting, failed, waiting on the owner, done.
    private func tone(for scene: LiveViewScene) -> Color {
        if let step = scene.step {
            return step.state.isFailed ? Theme.error : Theme.tally
        }
        switch timeline.activity {
        case .waitingForApproval, .waitingForAnswer, .locked: return Theme.warning
        case .finished(let outcome):
            switch outcome {
            case .success: return Theme.connected
            case .cancelled, .interrupted: return Theme.textTertiary
            case .failed, .blocked: return Theme.error
            }
        case .acting: return Theme.tally
        case .idle, .looking, .thinking, .usingTool: return Theme.textTertiary
        }
    }

    // MARK: Replay controls

    private var positionText: String {
        let numbers = timeline.stepNumbers
        guard !numbers.isEmpty else { return "No steps yet" }
        if replay.isFollowing { return "Live · \(numbers.count) \(numbers.count == 1 ? "step" : "steps")" }
        let index = replay.current.flatMap { numbers.firstIndex(of: $0) } ?? (numbers.count - 1)
        return "Step \(index + 1) of \(numbers.count)"
    }

    private var controls: some View {
        let numbers = timeline.stepNumbers
        let hasSteps = !numbers.isEmpty
        return VStack(spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                controlButton(.stepBack, label: "Previous step", identifier: "run.live.back", disabled: !hasSteps) {
                    replay.stepBack(numbers: numbers, now: Self.now())
                }
                controlButton(replay.isPlaying ? .pause : .play, label: replay.isPlaying ? "Pause replay" : "Play replay",
                              identifier: "run.live.play", disabled: !hasSteps) {
                    replay.togglePlay(numbers: numbers, now: Self.now())
                }
                controlButton(.stepForward, label: "Next step", identifier: "run.live.forward", disabled: !hasSteps) {
                    replay.stepForward(numbers: numbers, now: Self.now())
                }
                Slider(
                    value: Binding(
                        get: { replay.fraction(numbers: numbers) },
                        set: { replay.scrub(fraction: $0, numbers: numbers, now: Self.now()) }
                    ),
                    in: 0...1
                )
                .tint(Theme.tally)
                .disabled(numbers.count < 2)
                .accessibilityLabel("Replay position")
                .accessibilityValue(positionText)
                .accessibilityIdentifier("run.live.scrubber")
            }
            HStack(spacing: Theme.gapSmall) {
                Text(positionText)
                    .font(Theme.monoSmall)
                    .foregroundStyle(Theme.textTertiary)
                    .accessibilityHidden(true)
                Spacer(minLength: 4)
                speedButton
                liveChip(numbers: numbers)
            }
            .padding(.leading, 6)
        }
    }

    private func controlButton(_ icon: JarvisIcon, label: String, identifier: String, disabled: Bool,
                               action: @escaping @MainActor () -> Void) -> some View {
        Button(action: action) {
            Image.decorative(icon, size: Theme.iconStandard)
                .foregroundStyle(disabled ? Theme.textUnavailable : Theme.textPrimary)
                .frame(width: Theme.minTarget, height: Theme.minTarget)
                .modifier(Bezeled(fill: Theme.secondaryFill, radius: Theme.radiusElement, stroke: Theme.controlBezel))
                .contentShape(RoundedRectangle(cornerRadius: Theme.radiusElement, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
    }

    private var speedButton: some View {
        Button {
            replay.setSpeed(replay.speed.next, now: Self.now())
        } label: {
            Text(replay.speed.title)
                .font(Theme.mono.weight(.semibold))
                .foregroundStyle(Theme.textPrimary)
                .padding(.horizontal, 10)
                .frame(minHeight: 28)
                .background(Capsule().fill(Theme.raised))
                .overlay(Capsule().strokeBorder(Theme.bezel, lineWidth: Theme.bezelWidth))
                .frame(minHeight: Theme.minTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Replay speed")
        .accessibilityValue(replay.speed.title)
        .accessibilityIdentifier("run.live.speed")
    }

    private func liveChip(numbers: [Int]) -> some View {
        let following = replay.isFollowing
        return Button {
            replay.goLive(numbers: numbers, now: Self.now())
        } label: {
            HStack(spacing: 6) {
                Circle()
                    .fill(following ? Theme.tally : Theme.textTertiary)
                    .frame(width: 6, height: 6)
                Text("Live").font(Theme.meta.weight(.semibold))
            }
            .foregroundStyle(following ? Theme.textPrimary : Theme.textSecondary)
            .padding(.horizontal, 10)
            .frame(minHeight: 28)
            .background(Capsule().fill(following ? Theme.tally.opacity(0.16) : Theme.raised))
            .overlay(Capsule().strokeBorder(following ? Theme.tally.opacity(0.5) : Theme.bezel, lineWidth: Theme.bezelWidth))
            .frame(minHeight: Theme.minTarget)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Follow live")
        .accessibilityAddTraits(following ? .isSelected : [])
        .accessibilityIdentifier("run.live.followLive")
    }
}

// MARK: - The phone

/// The phone-shaped screen: backdrop, target outline, gesture marks, cursor and caption.
/// One accessibility element whose value is the caption.
private struct PhoneScreen: View {
    var scene: LiveViewScene
    var tone: Color
    var image: UIImage?
    var now: Double
    var reduceMotion: Bool

    private var screenSize: ViewerSize { scene.screen?.size.usable ?? ViewerSize.fallbackPhone }

    var body: some View {
        ZStack(alignment: .bottom) {
            GeometryReader { proxy in
                let mapping = ViewerMapping(
                    screen: screenSize,
                    view: ViewerSize(width: Double(proxy.size.width), height: Double(proxy.size.height))
                )
                ZStack(alignment: .topLeading) {
                    Theme.ground
                    backdrop(mapping)
                    targetOutline(mapping)
                    gestureMarks(mapping)
                    cursor(mapping)
                }
                .frame(width: proxy.size.width, height: proxy.size.height, alignment: .topLeading)
            }
            .clipShape(RoundedRectangle(cornerRadius: Theme.viewerScreenRadius, style: .continuous))

            CaptionPill(text: scene.caption, tone: tone, pulsing: scene.isIdle)
                .padding(Theme.gapSmall)
        }
        .aspectRatio(CGFloat(screenSize.displayAspect), contentMode: .fit)
        .padding(Theme.viewerBezel)
        .background(RoundedRectangle(cornerRadius: Theme.viewerOuterRadius, style: .continuous).fill(Theme.raisedHigh))
        .overlay(
            RoundedRectangle(cornerRadius: Theme.viewerOuterRadius, style: .continuous)
                .strokeBorder(Theme.controlBezel, lineWidth: Theme.bezelWidth)
        )
        .frame(maxHeight: Theme.viewerMaxHeight)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Live view")
        .accessibilityValue(scene.accessibilityValue)
        .accessibilityIdentifier("run.live")
    }

    // MARK: Backdrop

    @ViewBuilder private func backdrop(_ mapping: ViewerMapping) -> some View {
        let content = mapping.contentRect.cgRect
        if let image {
            Image(uiImage: image)
                .resizable()
                .interpolation(.medium)
                .frame(width: content.width, height: content.height)
                .position(x: content.midX, y: content.midY)
        } else if scene.screen?.source == .locked {
            VStack(spacing: Theme.gapSmall) {
                Image.decorative(.lock, size: Theme.iconDisplay)
                Text("Locked").font(Theme.meta.weight(.semibold))
            }
            .foregroundStyle(Theme.textTertiary)
            .position(x: content.midX, y: content.midY)
        } else if let elements = scene.screen?.elements, !elements.isEmpty {
            WireframeCanvas(elements: elements, mapping: mapping)
        }
    }

    // MARK: Target and gesture marks

    @ViewBuilder private func targetOutline(_ mapping: ViewerMapping) -> some View {
        if let step = scene.step, let target = step.target, let rect = mapping.toView(target.frame) {
            let color = step.state.isFailed ? Theme.error : Theme.tally
            let arrived = scene.pose.phase != .gliding
            let r = rect.cgRect.insetBy(dx: -3, dy: -3)
            RoundedRectangle(cornerRadius: Theme.radiusInner, style: .continuous)
                .fill(color.opacity(arrived ? 0.14 : 0.06))
                .overlay(
                    RoundedRectangle(cornerRadius: Theme.radiusInner, style: .continuous)
                        .strokeBorder(color.opacity(arrived ? 1 : 0.5), lineWidth: 2)
                )
                .frame(width: r.width, height: r.height)
                .position(x: r.midX, y: r.midY)
                .accessibilityHidden(true)
        }
    }

    @ViewBuilder private func gestureMarks(_ mapping: ViewerMapping) -> some View {
        if let step = scene.step {
            let pose = scene.pose
            let at = mapping.toView(pose.position).cgPoint
            switch step.gesture {
            case .swipe(let from, let to, _):
                if pose.phase == .swiping || pose.phase == .resting {
                    let start = mapping.toView(from).cgPoint
                    let end = pose.phase == .resting ? mapping.toView(to).cgPoint : at
                    Path { path in
                        path.move(to: start)
                        path.addLine(to: end)
                    }
                    .stroke(Theme.tally.opacity(0.45), style: StrokeStyle(lineWidth: Theme.swipeTrailWidth, lineCap: .round))
                    Circle()
                        .fill(Theme.tally.opacity(0.6))
                        .frame(width: Theme.swipeTrailWidth, height: Theme.swipeTrailWidth)
                        .position(start)
                }
            case .tap:
                if pose.phase == .pressing {
                    let progress = CGFloat(pose.progress)
                    // Reduce Motion: a fading ring of fixed size instead of an expanding one.
                    let diameter = reduceMotion
                        ? Theme.rippleStart + Theme.rippleGrowth / 2
                        : Theme.rippleStart + Theme.rippleGrowth * progress
                    Circle()
                        .stroke(Theme.tally.opacity(Double(1 - progress)), lineWidth: 2.5)
                        .frame(width: diameter, height: diameter)
                        .position(at)
                    Circle()
                        .fill(Theme.tally.opacity(0.5))
                        .frame(width: Theme.rippleStart * 0.7, height: Theme.rippleStart * 0.7)
                        .position(at)
                }
            case .longPress:
                if pose.phase == .pressing {
                    ZStack {
                        Circle()
                            .stroke(Theme.tally.opacity(0.25), lineWidth: 3)
                        Circle()
                            .trim(from: 0, to: CGFloat(pose.progress))
                            .stroke(Theme.tally, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                            .rotationEffect(.degrees(-90))
                    }
                    .frame(width: Theme.pressRing, height: Theme.pressRing)
                    .position(at)
                }
            case .type, .key, .launch, .openURL, .wait:
                EmptyView()
            }
        }
    }

    // MARK: Cursor

    private func cursor(_ mapping: ViewerMapping) -> some View {
        let tip = mapping.toView(scene.pose.position).cgPoint
        return CursorPointer(pressed: scene.pose.isPressed, idle: scene.isIdle, now: now, reduceMotion: reduceMotion)
            .fixedSize()
            .offset(x: tip.x, y: tip.y)
            .accessibilityIdentifier("run.live.cursor")
    }
}

/// The wireframe backdrop: one rounded rectangle per shown element, tinted by kind, with its
/// label when there is room.
private struct WireframeCanvas: View {
    var elements: [ViewerElement]
    var mapping: ViewerMapping

    var body: some View {
        Canvas { context, _ in
            for element in elements {
                guard let mapped = mapping.toView(element.frame) else { continue }
                let rect = mapped.cgRect
                guard rect.width >= 1, rect.height >= 1 else { continue }
                let style = WireframeStyle(kind: element.kind)
                let radius = min(Theme.radiusInner, min(rect.width, rect.height) / 3)
                let shape = Path(roundedRect: rect, cornerRadius: radius, style: .continuous)
                if style.fillOpacity > 0 {
                    context.fill(shape, with: .color(style.color.opacity(style.fillOpacity)))
                }
                if style.strokeOpacity > 0 {
                    context.stroke(shape, with: .color(style.color.opacity(style.strokeOpacity)), lineWidth: 0.75)
                }
                if let label = element.label, element.kind != .container, rect.height >= 8, rect.width >= 20 {
                    let size = min(Theme.wireframeLabelMax, max(7, rect.height * 0.5))
                    let text = Text(label)
                        .font(.system(size: size, weight: element.kind == .text ? .regular : .medium))
                        .foregroundStyle(element.kind == .text ? Theme.textSecondary : Theme.textSoft)
                    let inset = max(0, (rect.height - size * 1.4) / 2)
                    context.draw(text, in: rect.insetBy(dx: min(4, rect.width / 4), dy: inset))
                }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// How each kind of element is drawn on the wireframe.
private struct WireframeStyle {
    var color: Color
    var fillOpacity: Double
    var strokeOpacity: Double

    init(kind: ViewerElementKind) {
        switch kind {
        case .control: self.init(color: Theme.textSoft, fillOpacity: 0.10, strokeOpacity: 0.30)
        case .input: self.init(color: Theme.textPrimary, fillOpacity: 0.06, strokeOpacity: 0.45)
        case .text: self.init(color: Theme.textSoft, fillOpacity: 0, strokeOpacity: 0)
        case .image: self.init(color: Theme.textTertiary, fillOpacity: 0.14, strokeOpacity: 0.2)
        case .container: self.init(color: Theme.textPrimary, fillOpacity: 0.025, strokeOpacity: 0.08)
        case .alert: self.init(color: Theme.warning, fillOpacity: 0.08, strokeOpacity: 0.35)
        case .keyboard: self.init(color: Theme.textTertiary, fillOpacity: 0.12, strokeOpacity: 0.15)
        }
    }

    init(color: Color, fillOpacity: Double, strokeOpacity: Double) {
        self.color = color
        self.fillOpacity = fillOpacity
        self.strokeOpacity = strokeOpacity
    }
}

/// An arrow pointer with a "Jarvis" name tag, like a multiplayer cursor. Its top-left corner
/// is the arrow's tip.
private struct CursorPointer: View {
    var pressed: Bool
    var idle: Bool
    var now: Double
    var reduceMotion: Bool

    /// The idle pulse (1.4 s, like the status dot); none under Reduce Motion.
    private var pulse: Double {
        guard idle, !reduceMotion else { return 1 }
        return 0.8 + 0.2 * cos(now * 2 * Double.pi / Theme.pulsePeriod)
    }

    var body: some View {
        HStack(alignment: .top, spacing: 1) {
            PointerArrow()
                .fill(Theme.tally)
                .overlay(PointerArrow().stroke(Theme.textPrimary, style: StrokeStyle(lineWidth: 1.5, lineJoin: .round)))
                .frame(width: Theme.cursorHeight * 0.64, height: Theme.cursorHeight)
            Text("Jarvis")
                .font(Theme.cursorTagFont)
                .foregroundStyle(Theme.primaryText)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(Capsule().fill(Theme.tally))
                .padding(.top, Theme.cursorHeight * 0.7)
        }
        .scaleEffect(pressed ? 0.88 : 1, anchor: .topLeading)
        .opacity(pulse)
    }
}

/// The classic arrow pointer, tip at the top-left of its rectangle.
private struct PointerArrow: Shape {
    func path(in rect: CGRect) -> Path {
        // Drawn on a 12.6 × 19.6 grid, scaled to the rectangle.
        let sx = rect.width / 12.6
        let sy = rect.height / 19.6
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * sx, y: rect.minY + y * sy) }
        var path = Path()
        path.move(to: p(0, 0))
        path.addLine(to: p(0, 16.5))
        path.addLine(to: p(4.2, 12.6))
        path.addLine(to: p(7.0, 19.6))
        path.addLine(to: p(9.8, 18.4))
        path.addLine(to: p(7.1, 11.6))
        path.addLine(to: p(12.6, 11.6))
        path.closeSubpath()
        return path
    }
}

/// The caption at the bottom of the screen: a status dot and what Jarvis does.
private struct CaptionPill: View {
    var text: String
    var tone: Color
    var pulsing: Bool

    var body: some View {
        HStack(spacing: 6) {
            StatusDot(color: tone, pulsing: pulsing)
            Text(text)
                .font(.system(.footnote, weight: .semibold))
                .foregroundStyle(Theme.textPrimary)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: Theme.radiusField, style: .continuous).fill(Theme.raised.opacity(0.94)))
        .overlay(
            RoundedRectangle(cornerRadius: Theme.radiusField, style: .continuous)
                .strokeBorder(Theme.bezel, lineWidth: Theme.bezelWidth)
        )
        .accessibilityIdentifier("run.live.caption")
    }
}

/// The decoded image of the frame being shown (decoding once per frame key, not per
/// animation frame). A frame that is no longer in the store is not shown.
@MainActor
final class FrameImageCache {
    private var key: ViewerFrameKey?
    private var cached: UIImage?

    func image(for key: ViewerFrameKey?, in store: ViewerFrameStore) -> UIImage? {
        guard let key, let frame = store.frame(for: key) else {
            self.key = nil
            cached = nil
            return nil
        }
        if key != self.key {
            self.key = key
            cached = UIImage(data: frame.data)
        }
        return cached
    }
}

private extension ViewerRect {
    var cgRect: CGRect { CGRect(x: CGFloat(x), y: CGFloat(y), width: CGFloat(width), height: CGFloat(height)) }
}

private extension ViewerPoint {
    var cgPoint: CGPoint { CGPoint(x: CGFloat(x), y: CGFloat(y)) }
}
