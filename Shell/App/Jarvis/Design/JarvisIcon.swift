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
    case skills = "sparkles"
    case workflows = "arrow.triangle.branch"
    case history = "clock.arrow.circlepath"
    case away = "moon"
    case awayFilled = "moon.fill"
    case remote = "laptopcomputer.and.iphone"
    case wayScreen = "eye"
    case wayAppActions = "bolt"
    case wayDirect = "link"
    case trash = "trash"
    case edit = "pencil"
    case builtIn = "shippingbox"
    case phrase = "text.quote"
    case guardrail = "hand.raised"
    case proximity = "iphone.radiowaves.left.and.right"
    case warningOutline = "exclamationmark.triangle"
    case notify = "bell"
    case wait = "hourglass"
    case askOwner = "questionmark.circle"
    case condition = "questionmark.diamond"
    case shortcut = "square.stack.3d.up"
    case qrCode = "qrcode"
    case moveUp = "chevron.up"
    case remove = "minus.circle"
    case log = "list.bullet"
    case pause = "pause.fill"
    case stepBack = "backward.frame.fill"
    case stepForward = "forward.frame.fill"

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
