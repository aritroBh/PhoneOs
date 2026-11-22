import SwiftUI

/// Every icon the app draws: SF Symbols through one enum, so names live in one place.
/// Names follow Astryx's SF Symbol defaults (packages/native/src/icons/defaults.ts) where
/// it has one, else long-standing SF Symbols. Outlined by default; the filled names are
/// only used for semantic status, a selected tab, on/active state or primary emphasis.
enum JarvisIcon: String, CaseIterable {
    // Astryx defaults
    case close = "xmark"
    case chevronRight = "chevron.right"
    case chevronDown = "chevron.down"
    case check = "checkmark"
    case success = "checkmark.circle.fill"
    case warning = "exclamationmark.triangle.fill"
    case error = "exclamationmark.circle.fill"
    case info = "info.circle.fill"
    case clock = "clock"
    case search = "magnifyingglass"
    case send = "arrow.up"
    case stop = "stop.fill"
    case microphone = "mic"
    case more = "ellipsis"
    case wrench = "wrench.adjustable"

    // Long-standing SF Symbols
    case ask = "message"
    case apps = "square.grid.2x2"
    case rules = "slider.horizontal.3"
    case playground = "hand.tap"
    case replay = "arrow.counterclockwise"
    case runner = "iphone"
    case model = "laptopcomputer"
    case lock = "lock"
    case shield = "shield"
    case denied = "xmark.shield"
    case hollowDot = "circle"
    case plus = "plus"
    case play = "play.fill"

    /// The symbol as an image (scale it with `.font(.system(size:))`).
    var image: Image { Image(systemName: rawValue) }
}

extension Image {
    /// A decorative icon at a size role (hidden from VoiceOver: the adjacent text carries
    /// the meaning).
    static func decorative(_ icon: JarvisIcon, size: CGFloat = Theme.iconStandard) -> some View {
        icon.image
            .font(.system(size: size, weight: .semibold))
            .accessibilityHidden(true)
    }
}
