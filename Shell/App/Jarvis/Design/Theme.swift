import SwiftUI

/// Every colour, radius, size and duration the views use. Quiet graphite UI: colour only
/// means state, and the one accent ("tally") appears only while Jarvis is acting.
/// Views use these tokens only, never literals.
enum Theme {
    // MARK: Surfaces
    static let ground = Color(hex: 0x0E0E0F)
    static let surface = Color(hex: 0x18181A)
    static let raised = Color(hex: 0x1D1D20)
    static let raisedHigh = Color(hex: 0x242427)
    static let sheet = Color(hex: 0x1C1C1F)
    static let well = Color(hex: 0x131315)

    // MARK: Text
    static let textPrimary = Color(hex: 0xFAFAFA)
    static let textSecondary = Color(hex: 0xA6A6A6)
    static let textTertiary = Color(hex: 0x8F8F8F)
    static let textSoft = Color(hex: 0xC9C9C9)

    // MARK: State
    /// Acting (the only accent).
    static let tally = Color(hex: 0xFF7A1A)
    /// Needs you / warning.
    static let warning = Color(hex: 0xFFCE2F)
    static let warningFill = Color(hex: 0xFFCE2F).opacity(0.12)
    /// Blocked / error.
    static let error = Color(hex: 0xFF705D)
    static let errorText = Color(hex: 0xFF8E7E)
    static let errorFill = Color(hex: 0xFF705D).opacity(0.14)
    /// Connected / success.
    static let connected = Color(hex: 0x7BC862)

    // MARK: Controls
    static let primaryFill = Color(hex: 0xFAFAFA)
    static let primaryText = Color(hex: 0x0E0E0F)
    static let secondaryFill = Color(hex: 0x1F1F22)
    static let secondaryFillPressed = Color(hex: 0x2A2A2E)

    // MARK: Bezel (1 px inner stroke; no shadows, gradients or blur on cards)
    static let bezel = Color.white.opacity(0.08)
    static let controlBezel = Color.white.opacity(0.10)
    static let selectedRing = Color(hex: 0xFAFAFA)
    static let bezelWidth: CGFloat = 1
    static let selectedRingWidth: CGFloat = 1.5

    // MARK: Radii (continuous corners)
    static let radiusInner: CGFloat = 6
    static let radiusElement: CGFloat = 10
    static let radiusButton: CGFloat = 16
    static let radiusCard: CGFloat = 20
    static let radiusComposer: CGFloat = 28
    static let radiusSheet: CGFloat = 28

    // MARK: Spacing and sizes
    static let gutter: CGFloat = 16
    static let gap: CGFloat = 12
    static let gapSmall: CGFloat = 8
    static let minTarget: CGFloat = 44
    static let composerHeight: CGFloat = 56
    static let buttonHeight: CGFloat = 52
    static let statusTile: CGFloat = 32
    static let dot: CGFloat = 8

    // MARK: Icon size roles (Astryx iconography)
    static let iconCompact: CGFloat = 12
    static let iconStandard: CGFloat = 16
    static let iconProminent: CGFloat = 20
    static let iconDisplay: CGFloat = 24

    // MARK: Type
    static let largeTitle = Font.system(size: 34, weight: .semibold)
    static let largeTitleTracking: CGFloat = -0.85 // -0.025 em at 34 pt
    static let runTitle = Font.system(size: 28, weight: .semibold)
    static let runTitleTracking: CGFloat = -0.7
    static let question = Font.system(size: 26, weight: .semibold)
    static let rowTitle = Font.system(size: 16, weight: .semibold)
    static let body = Font.system(size: 15)
    static let meta = Font.system(size: 13)
    static let tab = Font.system(size: 11, weight: .semibold)
    static let mono = Font.system(size: 12).monospaced()
    static let monoSmall = Font.system(size: 11).monospaced()
    static let sectionLabel = Font.system(size: 13, weight: .semibold)

    // MARK: Motion (ease-out; Reduce Motion disables movement)
    static let fast = Animation.easeOut(duration: 0.125)
    static let medium = Animation.easeOut(duration: 0.3)
    static let pulsePeriod: Double = 1.4
}

extension Color {
    /// A colour from 0xRRGGBB (sRGB).
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}
