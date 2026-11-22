import SwiftUI
import JarvisKit

// Small building blocks shared by the screens. Tokens only (Theme, JarvisIcon).

/// A surface with the 1 px inner bezel. No shadow, gradient or blur.
struct Bezeled: ViewModifier {
    var fill: Color = Theme.surface
    var radius: CGFloat = Theme.radiusCard
    var stroke: Color = Theme.bezel

    func body(content: Content) -> some View {
        content
            .background(RoundedRectangle(cornerRadius: radius, style: .continuous).fill(fill))
            .overlay(
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(stroke, lineWidth: Theme.bezelWidth)
            )
    }
}

extension View {
    func card(fill: Color = Theme.surface, radius: CGFloat = Theme.radiusCard) -> some View {
        modifier(Bezeled(fill: fill, radius: radius))
    }

    /// The app's ground behind a screen.
    func screenBackground() -> some View {
        background(Theme.ground.ignoresSafeArea())
    }
}

/// An 8 pt status dot that pulses (1.4 s) while `pulsing`, unless Reduce Motion is on.
struct StatusDot: View {
    var color: Color
    var pulsing = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var dim = false

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: Theme.dot, height: Theme.dot)
            .opacity(pulsing && dim ? 0.45 : 1)
            .scaleEffect(pulsing && dim ? 0.8 : 1)
            .onAppear { startPulse() }
            .onChange(of: pulsing) { _, _ in startPulse() }
            .accessibilityHidden(true)
    }

    private func startPulse() {
        guard pulsing, !reduceMotion else {
            dim = false
            return
        }
        withAnimation(.easeInOut(duration: Theme.pulsePeriod / 2).repeatForever(autoreverses: true)) {
            dim = true
        }
    }
}

/// Dot + text: status never relies on colour alone.
struct StatusPill: View {
    var status: LiveRun.Status

    var body: some View {
        HStack(spacing: 6) {
            StatusDot(color: status.color, pulsing: status.pulses)
            Text(status.title)
                .font(Theme.meta.weight(.semibold))
                .foregroundStyle(Theme.textPrimary)
        }
        .padding(.horizontal, 10)
        .frame(minHeight: 28)
        .background(Capsule().fill(Theme.raised))
        .overlay(Capsule().strokeBorder(Theme.bezel, lineWidth: Theme.bezelWidth))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("status.pill")
    }
}

extension LiveRun.Status {
    var color: Color {
        switch self {
        case .standingBy: Theme.textTertiary
        case .driving, .replaying: Theme.tally
        case .needsYou, .locked: Theme.warning
        case .done: Theme.connected
        case .stopped: Theme.error
        }
    }

    var pulses: Bool {
        switch self {
        case .driving, .replaying, .needsYou, .locked: true
        default: false
        }
    }
}

/// White primary button (52 pt, radius 16).
struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Theme.rowTitle)
            .foregroundStyle(Theme.primaryText)
            .frame(maxWidth: .infinity, minHeight: Theme.buttonHeight)
            .background(
                RoundedRectangle(cornerRadius: Theme.radiusButton, style: .continuous)
                    .fill(Theme.primaryFill.opacity(configuration.isPressed ? 0.85 : 1))
            )
            .contentShape(RoundedRectangle(cornerRadius: Theme.radiusButton, style: .continuous))
            .animation(Theme.fast, value: configuration.isPressed)
    }
}

/// Graphite secondary button with a control bezel.
struct SecondaryButtonStyle: ButtonStyle {
    var foreground: Color = Theme.textPrimary

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Theme.rowTitle)
            .foregroundStyle(foreground)
            .frame(maxWidth: .infinity, minHeight: Theme.buttonHeight)
            .modifier(Bezeled(
                fill: configuration.isPressed ? Theme.secondaryFillPressed : Theme.secondaryFill,
                radius: Theme.radiusButton,
                stroke: Theme.controlBezel
            ))
            .contentShape(RoundedRectangle(cornerRadius: Theme.radiusButton, style: .continuous))
            .animation(Theme.fast, value: configuration.isPressed)
    }
}

/// A capsule chip: icon (optional) + text on a tinted fill.
struct Chip: View {
    var text: String
    var icon: JarvisIcon?
    var foreground: Color = Theme.textPrimary
    var fill: Color = Theme.raised

    var body: some View {
        HStack(spacing: 6) {
            if let icon { Image.decorative(icon, size: Theme.iconCompact) }
            Text(text).font(Theme.meta.weight(.semibold))
        }
        .foregroundStyle(foreground)
        .padding(.horizontal, 10)
        .frame(minHeight: 26)
        .background(Capsule().fill(fill))
        .overlay(Capsule().strokeBorder(Theme.bezel, lineWidth: Theme.bezelWidth))
        .accessibilityElement(children: .combine)
    }
}

/// A section label above a card ("Learned flows", "Always ask before"). No icon.
struct SectionLabel: View {
    var text: String

    var body: some View {
        Text(text)
            .font(Theme.sectionLabel)
            .foregroundStyle(Theme.textSecondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 4)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A screen's large title with an optional subtitle.
struct ScreenTitle: View {
    var title: String
    var subtitle: String?
    /// The title's accessibility identifier (`<screen>.title`).
    var identifier: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(Theme.largeTitle)
                .tracking(Theme.largeTitleTracking)
                .foregroundStyle(Theme.textPrimary)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier(identifier)
            if let subtitle {
                Text(subtitle).font(Theme.body).foregroundStyle(Theme.textSecondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The 32 pt status tile of an Activity row: icon + colour (the row's text carries the
/// status in words too).
struct StatusTile: View {
    var status: TaskRecord.Status

    private var style: (icon: JarvisIcon, color: Color, fill: Color) {
        switch status {
        case .success: (.check, Theme.connected, Theme.raisedHigh)
        case .cancelled: (.stop, Theme.textSecondary, Theme.raisedHigh)
        case .failed: (.error, Theme.errorText, Theme.errorFill)
        case .blocked: (.denied, Theme.errorText, Theme.errorFill)
        // Not an error: the phone stayed locked; running it again after an unlock is fine.
        case .interrupted: (.lock, Theme.warning, Theme.warningFill)
        }
    }

    var body: some View {
        Image.decorative(style.icon, size: Theme.iconStandard)
            .foregroundStyle(style.color)
            .frame(width: Theme.statusTile, height: Theme.statusTile)
            .modifier(Bezeled(fill: style.fill, radius: Theme.radiusElement))
    }
}

extension TaskRecord.Status {
    var title: String {
        switch self {
        case .success: "Done"
        case .failed: "Failed"
        case .cancelled: "Stopped"
        case .blocked: "Blocked"
        case .interrupted: "Phone stayed locked"
        }
    }
}
