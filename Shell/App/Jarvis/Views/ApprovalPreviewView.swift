import SwiftUI
import UIKit
import JarvisCore
import JarvisKit

/// The approval sheet's picture: the screen Jarvis evaluated, phone-shaped, with the control
/// it will act on outlined in the Live view cursor's colour, and a small label saying
/// whether it is the real screen ("What Jarvis sees now") or an outline of it ("Layout
/// only"). Everything it shows is decided in `ApprovalPreview` (JarvisKit) and the core's
/// viewer; this view only draws. Tapping it opens it full screen with pinch zoom.
struct ApprovalPreviewThumbnail: View {
    var preview: ApprovalPreview
    var screen: CoreViewerScreen?
    var image: UIImage?
    var onOpen: @MainActor () -> Void

    var body: some View {
        Button(action: onOpen) {
            ApprovalScreenPicture(preview: preview, screen: screen, image: image)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(preview.accessibilityLabel)
        .accessibilityValue(preview.badge)
        .accessibilityHint("Shows the screen full size")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("confirm.preview")
    }
}

/// The phone-shaped picture: backdrop (real screen or wireframe), the mark, the label.
struct ApprovalScreenPicture: View {
    var preview: ApprovalPreview
    var screen: CoreViewerScreen?
    var image: UIImage?

    var body: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            let height = proxy.size.height
            ZStack(alignment: .topLeading) {
                Theme.ground
                if let image {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.medium)
                        .scaledToFit()
                        .frame(width: width, height: height)
                } else if let elements = screen?.elements, !elements.isEmpty {
                    ApprovalWireframe(elements: elements, preview: preview)
                }
                ApprovalMarkView(mark: preview.mark, width: width, height: height)
            }
            .frame(width: width, height: height, alignment: .topLeading)
        }
        .aspectRatio(CGFloat(preview.aspectRatio), contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: Theme.viewerScreenRadius, style: .continuous))
        .overlay(alignment: .topLeading) {
            Text(preview.badge)
                .font(Theme.meta.weight(.semibold))
                .foregroundStyle(Theme.textPrimary)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(Capsule().fill(Theme.raised.opacity(0.94)))
                .overlay(Capsule().strokeBorder(Theme.bezel, lineWidth: Theme.bezelWidth))
                .padding(Theme.gapSmall)
                .accessibilityIdentifier("confirm.preview.badge")
        }
        .padding(Theme.viewerBezel)
        .background(RoundedRectangle(cornerRadius: Theme.viewerOuterRadius, style: .continuous).fill(Theme.raisedHigh))
        .overlay(
            RoundedRectangle(cornerRadius: Theme.viewerOuterRadius, style: .continuous)
                .strokeBorder(Theme.controlBezel, lineWidth: Theme.bezelWidth)
        )
    }
}

/// The target: an outline (as the Live view outlines a step's target), a ring with a
/// crosshair for a point, or nothing.
private struct ApprovalMarkView: View {
    var mark: ApprovalPreview.Mark
    var width: CGFloat
    var height: CGFloat

    var body: some View {
        switch mark {
        case .outline(let unit):
            if let placed = ApprovalPreview.place(unit, width: Double(width), height: Double(height)) {
                let r = CGRect(x: CGFloat(placed.x), y: CGFloat(placed.y), width: CGFloat(placed.width),
                               height: CGFloat(placed.height)).insetBy(dx: -3, dy: -3)
                RoundedRectangle(cornerRadius: Theme.radiusInner, style: .continuous)
                    .fill(Theme.tally.opacity(0.14))
                    .overlay(
                        RoundedRectangle(cornerRadius: Theme.radiusInner, style: .continuous)
                            .strokeBorder(Theme.tally, lineWidth: 2)
                    )
                    .frame(width: r.width, height: r.height)
                    .position(x: r.midX, y: r.midY)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
            }
        case .point(let unit):
            let x = CGFloat(unit.x) * width
            let y = CGFloat(unit.y) * height
            ZStack {
                Circle()
                    .stroke(Theme.tally, lineWidth: 2)
                    .frame(width: Theme.pressRing * 0.7, height: Theme.pressRing * 0.7)
                Path { path in
                    path.move(to: CGPoint(x: 0, y: Theme.pressRing / 2))
                    path.addLine(to: CGPoint(x: Theme.pressRing, y: Theme.pressRing / 2))
                    path.move(to: CGPoint(x: Theme.pressRing / 2, y: 0))
                    path.addLine(to: CGPoint(x: Theme.pressRing / 2, y: Theme.pressRing))
                }
                .stroke(Theme.tally, lineWidth: 1.5)
            }
            .frame(width: Theme.pressRing, height: Theme.pressRing)
            .position(x: x, y: y)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        case .none:
            EmptyView()
        }
    }
}

/// The wireframe backdrop of the evaluated screen, drawn like the Live view's.
private struct ApprovalWireframe: View {
    var elements: [CoreViewerElement]
    var preview: ApprovalPreview

    var body: some View {
        Canvas { context, size in
            for element in elements {
                guard let unit = preview.unit(element.frame),
                      let placed = ApprovalPreview.place(unit, width: Double(size.width), height: Double(size.height)) else { continue }
                let rect = CGRect(x: CGFloat(placed.x), y: CGFloat(placed.y), width: CGFloat(placed.width),
                                  height: CGFloat(placed.height))
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
                    let fontSize = min(Theme.wireframeLabelMax, max(7, rect.height * 0.5))
                    let text = Text(label)
                        .font(.system(size: fontSize, weight: element.kind == .text ? .regular : .medium))
                        .foregroundStyle(element.kind == .text ? Theme.textSecondary : Theme.textSoft)
                    let inset = max(0, (rect.height - fontSize * 1.4) / 2)
                    context.draw(text, in: rect.insetBy(dx: min(4, rect.width / 4), dy: inset))
                }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

/// The picture full screen: pinch to zoom (1×–5×), double tap to reset, Done to close.
/// Under Reduce Motion the zoom resets without animation.
struct ApprovalPreviewFullScreen: View {
    var preview: ApprovalPreview
    var screen: CoreViewerScreen?
    var image: UIImage?
    var onClose: @MainActor () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var zoom: CGFloat = 1
    @State private var settledZoom: CGFloat = 1

    var body: some View {
        VStack(spacing: Theme.gap) {
            HStack {
                Text(preview.badge)
                    .font(Theme.sectionLabel)
                    .foregroundStyle(Theme.textSoft)
                Spacer()
                Button(action: onClose) {
                    Image.decorative(.close, size: Theme.iconStandard)
                        .foregroundStyle(Theme.textPrimary)
                        .frame(width: Theme.minTarget, height: Theme.minTarget)
                        .modifier(Bezeled(fill: Theme.secondaryFill, radius: Theme.radiusElement, stroke: Theme.controlBezel))
                        .contentShape(RoundedRectangle(cornerRadius: Theme.radiusElement, style: .continuous))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Done")
                .accessibilityIdentifier("confirm.preview.close")
            }
            ApprovalScreenPicture(preview: preview, screen: screen, image: image)
                .scaleEffect(zoom)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .clipped()
                .contentShape(Rectangle())
                .gesture(
                    MagnifyGesture()
                        .onChanged { value in
                            zoom = min(5, max(1, settledZoom * value.magnification))
                        }
                        .onEnded { _ in
                            settledZoom = zoom
                        }
                )
                .onTapGesture(count: 2) {
                    if reduceMotion {
                        zoom = 1
                    } else {
                        withAnimation(Theme.medium) { zoom = 1 }
                    }
                    settledZoom = 1
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(preview.accessibilityLabel)
                .accessibilityValue(preview.badge)
                .accessibilityIdentifier("confirm.preview.full")
            Text(preview.caption)
                .font(Theme.meta)
                .foregroundStyle(Theme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(Theme.gutter)
        .background(Theme.ground)
    }
}

/// The decoded image of the approval frame (decoded once per key). A frame that is no
/// longer in the store is not shown.
@MainActor
final class ApprovalImageCache {
    private var key: ViewerFrameKey?
    private var cached: UIImage?

    func image(for preview: ApprovalPreview, in store: ApprovalFrameStore) -> UIImage? {
        guard let frame = preview.frame(in: store) else {
            key = nil
            cached = nil
            return nil
        }
        if preview.key != key {
            key = preview.key
            cached = UIImage(data: frame.data)
        }
        return cached
    }
}
