import Foundation
import JarvisCore

/// Which process's tree is on top of the screen: the one decision the in-process XCUITest
/// runner (`XCUITestPhoneRunner`, Mac only) uses for its snapshot, for `frontBundleId()` and
/// for the check next to every gesture (docs/ARCHITECTURE.md, *Swift shell*, the top layer).
///
/// What real trees showed (iOS 26.5 simulator, probe runs 134 and 135): a system alert (the
/// notification permission prompt, the web sign-in consent prompt) is in SpringBoard's tree
/// only, while the app below stays in the foreground; Spotlight is its own process
/// (`com.apple.Spotlight`) whose tree holds the field and the results, while SpringBoard's
/// shows only the status bar. Reading "the known app in front, else SpringBoard" missed both.
///
/// The runner gathers the inputs with cheap XCUITest queries on the OS's processes
/// (SpringBoard's alerts, Spotlight's application state) and never reads the front app's
/// tree for it; this type only decides, so the decision is tested on Linux.
public enum TopLayer {
    /// SpringBoard's bundle id.
    public static let springBoard = "com.apple.springboard"
    /// Spotlight's bundle id (its own process on iOS 26).
    public static let spotlight = "com.apple.Spotlight"
    /// The identifier of Spotlight's search field (probe runs 134 and 135).
    public static let spotlightFieldIdentifier = "SpotlightSearchField"

    /// What is on top, and why.
    public struct Decision: Sendable, Equatable {
        /// The bundle id whose tree is read and against which a command's `expect` is checked.
        public var bundleId: String
        /// When the top layer is a SpringBoard alert over an app: that app (the foreground
        /// reading). `nil` otherwise, and for an alert over the Home Screen.
        public var alertOver: String?
        /// Top layer is a SpringBoard alert.
        public var systemAlert: Bool

        public init(bundleId: String, alertOver: String? = nil, systemAlert: Bool = false) {
            self.bundleId = bundleId
            self.alertOver = alertOver
            self.systemAlert = systemAlert
        }

        /// The observation note that says what the top layer is when it is not the
        /// foreground reading (`nil` when it is).
        public var note: String? {
            if systemAlert {
                if let over = alertOver {
                    return "a system alert (SpringBoard's) is up over \(over): this observation is SpringBoard's tree, the alert"
                }
                return "a system alert (SpringBoard's) is up: this observation is SpringBoard's tree, the alert"
            }
            if bundleId == TopLayer.spotlight {
                return "Spotlight (\(TopLayer.spotlight)) is in front: this observation is Spotlight's tree"
            }
            return nil
        }
    }

    /// The top layer:
    /// 1. a SpringBoard alert is up → SpringBoard (the alert), over `foreground`;
    /// 2. else Spotlight is in front → `com.apple.Spotlight`;
    /// 3. else `foreground` (the known app in front, else SpringBoard: the runner's reading).
    ///
    /// - Parameters:
    ///   - systemAlertUp: SpringBoard's tree holds an alert (a query on SpringBoard).
    ///   - spotlightInFront: Spotlight runs in the foreground and shows its search field
    ///     (queries on the Spotlight process).
    ///   - foreground: the runner's foreground reading (never empty; SpringBoard when no
    ///     known app is in front).
    public static func decide(systemAlertUp: Bool, spotlightInFront: Bool, foreground: String) -> Decision {
        if systemAlertUp {
            let over = isSpringBoard(foreground) ? nil : foreground
            return Decision(bundleId: springBoard, alertOver: over, systemAlert: true)
        }
        if spotlightInFront {
            return Decision(bundleId: spotlight)
        }
        return Decision(bundleId: foreground)
    }

    /// Whether `bundleId` is SpringBoard (case and surrounding spaces ignored).
    public static func isSpringBoard(_ bundleId: String) -> Bool {
        bundleId.trimmingCharacters(in: .whitespaces).caseInsensitiveCompare(springBoard) == .orderedSame
    }
}

/// A notification banner over an app (iOS 26.5, probe run 134: `banner_springboard.txt`, the
/// banner is `Other(Type83) id=NotificationShortLookView frame {8, 59, 424, 66}` in
/// SpringBoard's tree; `banner_front_settings.txt`, nothing of it in the front app's tree): a
/// tap near the top of the app can land on the banner and open the notifying app.
///
/// The runner adds to the front app's typed tree one synthetic node per banner
/// (`node(frame:)`): the banner's frame in screen points, a control role, no label, value or
/// identifier, disabled, not hittable — the shape of the core's Home Screen stubs — so the
/// core's covered-control rule marks what lies under it "(covered)" and refuses taps there,
/// and nothing of the notification's text reaches the model. At touch time a point inside a
/// banner that the command's look did not hold is refused (`refuses`).
public enum BannerCover {
    /// The OS's identifier of the banner node.
    public static let identifier = "NotificationShortLookView"

    /// The frames of every banner in SpringBoard's raw tree, in screen points. Some
    /// SpringBoard windows report their frames in pixels (`{1320, 2868}` on a 440 × 956 pt
    /// screen, the same capture): each banner's frame is converted by the window that holds
    /// it (`points(_:window:screen:)`), with SpringBoard's application frame as the screen.
    public static func frames(inSpringBoard raw: RawElement) -> [Rect] {
        let screen = raw.frame
        var out: [Rect] = []
        var stack: [(RawElement, Rect)] = [(raw, screen)]
        while let (e, window) = stack.popLast() {
            let here = e.elementType == XCUIElementTypeCode.window ? e.frame : window
            if e.identifier == identifier {
                out.append(points(e.frame, window: here, screen: screen))
                continue
            }
            for c in e.children.reversed() { stack.append((c, here)) }
        }
        return out
    }

    /// `frame` in screen points, given the frame of the window that holds it and the
    /// screen's frame in points (SpringBoard's application element). A window whose size is
    /// the screen's times one factor greater than 1 (both sides, within 1 %) reports pixels:
    /// the frame is divided by that factor. Otherwise it is returned as it is (a frame is
    /// never scaled by a constant).
    public static func points(_ frame: Rect, window: Rect, screen: Rect) -> Rect {
        guard screen.width > 0, screen.height > 0, window.width > 0, window.height > 0 else { return frame }
        let sx = window.width / screen.width
        let sy = window.height / screen.height
        guard sx > 1.01, abs(sx - sy) <= 0.01 * sx else { return frame }
        return Rect(x: frame.x / sx, y: frame.y / sx, width: frame.width / sx, height: frame.height / sx)
    }

    /// The synthetic node that stands for a banner in the front app's tree.
    public static func node(frame: Rect) -> UiNode {
        UiNode(
            role: .button, label: nil, value: nil, identifier: nil, placeholder: nil,
            frame: frame, enabled: false, visible: true, hittable: false,
            focused: false, selected: false, secure: false, children: []
        )
    }

    /// `root` with one `node(frame:)` per banner appended as its last children (drawn over
    /// everything else in the app's tree).
    public static func covering(_ root: UiNode, banners: [Rect]) -> UiNode {
        guard !banners.isEmpty else { return root }
        var out = root
        out.children += banners.map { node(frame: $0) }
        return out
    }

    /// The point a command touches first, if it touches the screen: a tap's or long press's
    /// point, a swipe's start. Typing and keys touch nothing.
    public static func touchPoint(_ command: RunnerCommand) -> (x: Double, y: Double)? {
        switch command {
        case .tap(let x, let y): return (x, y)
        case .longPress(let x, let y, _): return (x, y)
        case .swipe(let from, _, _): return (from.x, from.y)
        default: return nil
        }
    }

    /// Whether a touch at (`x`, `y`) is refused: a banner up now (`banners`, points) holds
    /// the point, and the look the command was planned on held no banner at about that frame
    /// (`planned`, the synthetic nodes the runner reported; 4 pt per edge tolerance, a
    /// banner settles as it slides in).
    public static func refuses(x: Double, y: Double, banners: [Rect], planned: [Rect]) -> Bool {
        banners.contains { b in
            contains(b, x, y) && !planned.contains { near($0, b) }
        }
    }

    static func contains(_ r: Rect, _ x: Double, _ y: Double) -> Bool {
        x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
    }

    static func near(_ a: Rect, _ b: Rect) -> Bool {
        let tolerance = 4.0
        return abs(a.x - b.x) <= tolerance && abs(a.y - b.y) <= tolerance
            && abs(a.width - b.width) <= tolerance && abs(a.height - b.height) <= tolerance
    }
}

extension RunnerResult {
    /// The command was refused because a notification banner now covers the point it
    /// touches and the look it was planned on did not show that banner (`BannerCover`);
    /// nothing was carried out.
    public static func staleBanner(expected: String) -> RunnerResult {
        RunnerResult(
            ok: false,
            error: "not carried out: a notification banner now covers the point, and the command was planned on \(expected) without it",
            failure: .stale
        )
    }
}
