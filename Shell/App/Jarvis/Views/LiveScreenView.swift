import SwiftUI
import UIKit
import JarvisCore
import JarvisKit

/// The Live view: a phone-shaped viewer of the screen Jarvis acts on, with a cursor that
/// glides to each target and shows the tap, long press or swipe, a caption, and replay
/// controls (step back, play, step forward, scrubber, speed, Live).
///
/// Everything it shows is decided in the core (`JarvisViewer`, through `LiveViewModel`):
/// once per frame of `TimelineView(.animation)` the scene says which screen and step to
/// show, the caption and tone, and where the cursor, target outline and swipe trail are in
/// view points. This view only draws. The `TimelineView` is paused whenever nothing moves
/// (the scene's `needsFrames`). The real screen is never drawn while the app is not active,
/// so the app switcher's snapshot never holds it.
///
/// iOS does not let an app draw over other apps, so this viewer lives inside Jarvis.
struct LiveScreenView: View {
    var viewer: LiveViewModel
    var timeline: CoreViewerTimeline
    var frames: ViewerFrameStore

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    /// The scrubber's state and texts, as the last replay call returned them.
    @State private var status: CoreReplayStatus?
    @State private var images = FrameImageCache()
    /// Bumped when the drawn scene stops needing frames, so `body` runs again and pauses the
    /// `TimelineView`.
    @State private var settleTick = 0
    /// This view is on screen (between `onAppear` and `onDisappear`).
    @State private var appeared = false
    /// The phone screen's size in points (the scene maps into it); zero until laid out.
    @State private var screenArea = CGSize.zero

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

            let paused = isPaused(settleTick)
            TimelineView(.animation(minimumInterval: nil, paused: paused)) { context in
                // A paused schedule may hand an old date: draw the present.
                let now = paused ? Self.now() : context.date.timeIntervalSinceReferenceDate
                // The one FFI call of this frame (the wireframe comes from the cache).
                let scene = viewer.scene(CoreSceneQuery(nowMs: now * 1_000, reduceMotion: reduceMotion,
                                                       viewWidth: Double(screenArea.width), viewHeight: Double(screenArea.height)))
                let screen = viewer.screen(id: scene?.screenId)
                PhoneScreen(
                    scene: scene,
                    screen: screen,
                    step: timeline.step(number: scene?.stepNumber),
                    tone: Self.color(scene?.tone),
                    // Never the real screen while the app is not active (app switcher).
                    image: scenePhase == .active ? images.image(for: viewer.frameKey(for: screen?.info), in: frames) : nil,
                    now: now,
                    reduceMotion: reduceMotion,
                    area: $screenArea
                )
                .onChange(of: scene?.needsFrames ?? false) { _, needs in
                    if !needs { settleTick &+= 1 }
                }
                .onChange(of: ReplayMark(scene)) { _, _ in
                    // Playback moved on, or steps arrived: the labels follow.
                    status = viewer.replayStatus()
                }
            }
            .frame(maxWidth: .infinity)

            controls
        }
        .padding(EdgeInsets(top: 14, leading: 10, bottom: 10, trailing: 10))
        .card(fill: Theme.well)
        .onAppear {
            status = viewer.replay(.reset, nowMs: Self.nowMs(), reduceMotion: reduceMotion)
            appeared = true
            reportShown()
        }
        .onDisappear {
            appeared = false
            reportShown()
        }
        .onChange(of: scenePhase) { _, _ in reportShown() }
        .onChange(of: timeline.taskId) { _, _ in
            // A new run: follow it live, whatever the last replay was doing.
            status = viewer.replay(.reset, nowMs: Self.nowMs(), reduceMotion: reduceMotion)
        }
    }

    private static func now() -> Double { Date().timeIntervalSinceReferenceDate }

    /// The core's clock: milliseconds.
    private static func nowMs() -> Double { now() * 1_000 }

    /// Whether the `TimelineView` is paused: nothing moves, or the app is not active.
    /// `tick` only makes `body` depend on `settleTick`.
    private func isPaused(_ tick: Int) -> Bool {
        guard scenePhase == .active else { return true }
        return !viewer.needsFrames(nowMs: Self.nowMs(), reduceMotion: reduceMotion)
    }

    /// Tells the frame store whether this Live view is on screen (a run that ends unwatched
    /// drops its real-screen frames).
    private func reportShown() {
        frames.isShown = appeared && scenePhase == .active
    }

    /// The caption dot's colour: acting, failed, waiting on the owner, done.
    private static func color(_ tone: CoreSceneTone?) -> Color {
        guard let tone else { return Theme.textTertiary }
        switch tone {
        case .acting: return Theme.tally
        case .waiting: return Theme.warning
        case .error: return Theme.error
        case .success: return Theme.connected
        case .neutral: return Theme.textTertiary
        }
    }

    // MARK: Replay controls

    /// A scrubber control; its returned state drives the labels.
    private func replay(_ action: CoreReplayAction) {
        status = viewer.replay(action, nowMs: Self.nowMs(), reduceMotion: reduceMotion)
    }

    private var positionText: String { status?.positionText ?? "No steps yet" }

    private var controls: some View {
        let count = timeline.steps.count
        let hasSteps = count > 0
        let playing = status?.playing ?? false
        return VStack(spacing: Theme.gapSmall) {
            HStack(spacing: Theme.gapSmall) {
                controlButton(.stepBack, label: "Previous step", identifier: "run.live.back", disabled: !hasSteps) {
                    replay(.stepBack)
                }
                controlButton(playing ? .pause : .play, label: playing ? "Pause replay" : "Play replay",
                              identifier: "run.live.play", disabled: !hasSteps) {
                    replay(.togglePlay)
                }
                controlButton(.stepForward, label: "Next step", identifier: "run.live.forward", disabled: !hasSteps) {
                    replay(.stepForward)
                }
                Slider(
                    value: Binding(
                        get: { status?.fraction ?? 1 },
                        set: { replay(.scrubFraction(fraction: $0)) }
                    ),
                    in: 0...1
                )
                .tint(Theme.tally)
                .disabled(count < 2)
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
                liveChip
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
        let title = status?.speedTitle ?? "1×"
        return Button {
            replay(.nextSpeed)
        } label: {
            Text(title)
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
        .accessibilityValue(title)
        .accessibilityIdentifier("run.live.speed")
    }

    private var liveChip: some View {
        let following = status?.following ?? true
        return Button {
            replay(.goLive)
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

/// What of the drawn scene the replay labels depend on (refreshed when it changes).
private struct ReplayMark: Equatable {
    var current: UInt32?
    var following: Bool
    var playing: Bool
    var count: UInt32

    init(_ scene: CoreLiveScene?) {
        current = scene?.replayCurrent
        following = scene?.following ?? true
        playing = scene?.playing ?? false
        count = scene?.positionCount ?? 0
    }
}

// MARK: - The phone

/// The phone-shaped screen: backdrop, target outline, gesture marks, cursor and caption.
/// One accessibility element whose value is the caption. Draws only the scene's values
/// (view points); nothing at all when the scene could not be had.
private struct PhoneScreen: View {
    var scene: CoreLiveScene?
    /// The backdrop's screen with its wireframe.
    var screen: CoreViewerScreen?
    /// The step shown.
    var step: CoreViewerStep?
    var tone: Color
    var image: UIImage?
    var now: Double
    var reduceMotion: Bool
    /// Reports the screen's size in points (what the scene maps into).
    @Binding var area: CGSize

    /// The screen's aspect ratio (an iPhone in portrait until there is a scene).
    private var displayAspect: Double { scene?.displayAspect ?? 393.0 / 852.0 }

    var body: some View {
        ZStack(alignment: .bottom) {
            GeometryReader { proxy in
                ZStack(alignment: .topLeading) {
                    Theme.ground
                    if let scene, scene.scale > 0 {
                        backdrop(scene)
                        targetOutline(scene)
                        gestureMarks(scene)
                        cursor(scene)
                    }
                }
                .frame(width: proxy.size.width, height: proxy.size.height, alignment: .topLeading)
                .onAppear { area = proxy.size }
                .onChange(of: proxy.size) { _, size in area = size }
            }
            .clipShape(RoundedRectangle(cornerRadius: Theme.viewerScreenRadius, style: .continuous))

            CaptionPill(text: scene?.caption ?? "", tone: tone, pulsing: scene?.isIdle ?? false)
                .padding(Theme.gapSmall)
        }
        .aspectRatio(CGFloat(displayAspect), contentMode: .fit)
        .padding(Theme.viewerBezel)
        .background(RoundedRectangle(cornerRadius: Theme.viewerOuterRadius, style: .continuous).fill(Theme.raisedHigh))
        .overlay(
            RoundedRectangle(cornerRadius: Theme.viewerOuterRadius, style: .continuous)
                .strokeBorder(Theme.controlBezel, lineWidth: Theme.bezelWidth)
        )
        .frame(maxHeight: Theme.viewerMaxHeight)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Live view")
        .accessibilityValue(scene?.accessibilityValue ?? "")
        .accessibilityIdentifier("run.live")
    }

    // MARK: Backdrop

    @ViewBuilder private func backdrop(_ scene: CoreLiveScene) -> some View {
        let content = scene.content.cgRect
        if let image {
            // Aspect-fit in the screen's rect: never stretched.
            Image(uiImage: image)
                .resizable()
                .interpolation(.medium)
                .scaledToFit()
                .frame(width: content.width, height: content.height)
                .position(x: content.midX, y: content.midY)
        } else if screen?.info.source == .locked {
            VStack(spacing: Theme.gapSmall) {
                Image.decorative(.lock, size: Theme.iconDisplay)
                Text("Locked").font(Theme.meta.weight(.semibold))
            }
            .foregroundStyle(Theme.textTertiary)
            .position(x: content.midX, y: content.midY)
        } else if let elements = screen?.elements, !elements.isEmpty {
            WireframeCanvas(elements: elements, scene: scene)
            if screen?.info.fromText == true {
                // The runner's text format is forgeable by on-screen text: say where the
                // layout came from.
                Text("Layout from the runner's text")
                    .font(Theme.meta)
                    .foregroundStyle(Theme.textTertiary)
                    .position(x: content.midX, y: content.minY + 14)
                    .accessibilityHidden(true)
            }
        }
    }

    // MARK: Target and gesture marks

    @ViewBuilder private func targetOutline(_ scene: CoreLiveScene) -> some View {
        if let rect = scene.targetInView {
            let color = step?.state.isFailed == true ? Theme.error : Theme.tally
            let arrived = scene.phase != .gliding
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

    @ViewBuilder private func gestureMarks(_ scene: CoreLiveScene) -> some View {
        if let step {
            let at = scene.cursorInView.cgPoint
            switch step.gesture {
            case .swipe:
                // The scene gives the trail while the finger drags and once it rests.
                if let start = scene.trailStart?.cgPoint, let end = scene.trailEnd?.cgPoint {
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
                if scene.phase == .pressing {
                    let progress = CGFloat(scene.progress)
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
                if scene.phase == .pressing {
                    ZStack {
                        Circle()
                            .stroke(Theme.tally.opacity(0.25), lineWidth: 3)
                        Circle()
                            .trim(from: 0, to: CGFloat(scene.progress))
                            .stroke(Theme.tally, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                            .rotationEffect(.degrees(-90))
                    }
                    .frame(width: Theme.pressRing, height: Theme.pressRing)
                    .position(at)
                }
            case .typeText, .key, .launch, .openUrl, .wait, .unknown:
                EmptyView()
            }
        }
    }

    // MARK: Cursor

    private func cursor(_ scene: CoreLiveScene) -> some View {
        let tip = scene.cursorInView.cgPoint
        return CursorPointer(pressed: scene.pressed, idle: scene.isIdle, now: now, reduceMotion: reduceMotion)
            .fixedSize()
            .offset(x: tip.x, y: tip.y)
            .accessibilityIdentifier("run.live.cursor")
    }
}

/// The wireframe backdrop: one rounded rectangle per shown element, tinted by kind, with its
/// label when there is room.
private struct WireframeCanvas: View {
    var elements: [CoreViewerElement]
    var scene: CoreLiveScene

    var body: some View {
        Canvas { context, _ in
            for element in elements {
                guard let mapped = scene.viewRect(element.frame) else { continue }
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

/// How each kind of element is drawn on the wireframe (also the approval sheet's).
struct WireframeStyle {
    var color: Color
    var fillOpacity: Double
    var strokeOpacity: Double

    init(kind: CoreViewerElementKind) {
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

private extension Rect {
    var cgRect: CGRect { CGRect(x: CGFloat(x), y: CGFloat(y), width: CGFloat(width), height: CGFloat(height)) }
}

private extension Point {
    var cgPoint: CGPoint { CGPoint(x: CGFloat(x), y: CGFloat(y)) }
}
