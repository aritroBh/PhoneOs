import Foundation

// Plain geometry for the Live view: platform-free (no CoreGraphics), `Codable` with stable
// keys, so the same values can later be streamed to the paired remote controller. Every
// function here is total: NaN, infinity, zero and negative sizes give a usable answer.

/// A point in screen points (or view points, after `ViewerMapping`).
public struct ViewerPoint: Sendable, Equatable, Hashable, Codable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }

    public static let zero = ViewerPoint(x: 0, y: 0)

    /// Both coordinates are finite.
    public var isFinite: Bool { x.isFinite && y.isFinite }

    /// Distance to `other` (0 when either point is not finite).
    public func distance(to other: ViewerPoint) -> Double {
        guard isFinite, other.isFinite else { return 0 }
        let d = (other.x - x) * (other.x - x) + (other.y - y) * (other.y - y)
        return d.isFinite ? d.squareRoot() : 0
    }

    /// The point `fraction` of the way to `other` (`fraction` clamped to 0…1).
    public func interpolated(to other: ViewerPoint, _ fraction: Double) -> ViewerPoint {
        let f = fraction.isFinite ? min(max(fraction, 0), 1) : 0
        if f >= 1 { return other }
        if f <= 0 { return self }
        return ViewerPoint(x: x + (other.x - x) * f, y: y + (other.y - y) * f)
    }

    enum CodingKeys: String, CodingKey { case x, y }
}

/// A width and height.
public struct ViewerSize: Sendable, Equatable, Hashable, Codable {
    public var width: Double
    public var height: Double

    public init(width: Double, height: Double) {
        self.width = width
        self.height = height
    }

    /// The size assumed when a screen reports none it can be drawn with (an iPhone 16/17
    /// in portrait).
    public static let fallbackPhone = ViewerSize(width: 393, height: 852)

    /// Both sides finite and positive.
    public var isUsable: Bool { width.isFinite && height.isFinite && width > 0 && height > 0 }

    /// Landscape (wider than tall).
    public var isLandscape: Bool { isUsable && width > height }

    /// This size when usable, otherwise `fallbackPhone`.
    public var usable: ViewerSize { isUsable ? self : Self.fallbackPhone }

    /// Width / height of `usable`, clamped to 0.3…3 so an absurd report cannot make the
    /// viewer a sliver.
    public var displayAspect: Double {
        let s = usable
        return min(max(s.width / s.height, 0.3), 3)
    }

    /// The centre of `usable`.
    public var center: ViewerPoint {
        let s = usable
        return ViewerPoint(x: s.width / 2, y: s.height / 2)
    }

    /// `point` moved inside `0…width × 0…height` (a non-finite coordinate becomes the
    /// centre's).
    public func clamp(_ point: ViewerPoint) -> ViewerPoint {
        let s = usable
        let x = point.x.isFinite ? min(max(point.x, 0), s.width) : s.width / 2
        let y = point.y.isFinite ? min(max(point.y, 0), s.height) : s.height / 2
        return ViewerPoint(x: x, y: y)
    }

    enum CodingKeys: String, CodingKey { case width, height }
}

/// An axis-aligned rectangle. Containment is half-open, like the core's `Rect`.
public struct ViewerRect: Sendable, Equatable, Hashable, Codable {
    public var x: Double
    public var y: Double
    public var width: Double
    public var height: Double

    public init(x: Double, y: Double, width: Double, height: Double) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }

    public static let zero = ViewerRect(x: 0, y: 0, width: 0, height: 0)

    public var isFinite: Bool { x.isFinite && y.isFinite && width.isFinite && height.isFinite }
    /// Finite with a positive width and height.
    public var hasArea: Bool { isFinite && width > 0 && height > 0 }
    public var maxX: Double { x + width }
    public var maxY: Double { y + height }
    public var center: ViewerPoint { ViewerPoint(x: x + width / 2, y: y + height / 2) }

    /// Half-open containment; an empty or non-finite rectangle contains nothing.
    public func contains(_ p: ViewerPoint) -> Bool {
        hasArea && p.isFinite && p.x >= x && p.x < maxX && p.y >= y && p.y < maxY
    }

    /// Where content of `size` lands when it is fitted into this rectangle keeping its
    /// aspect ratio, centred (letterboxed, never stretched or cropped). Content without a
    /// usable size, or a rectangle without area, gives this rectangle.
    public func aspectFit(_ size: ViewerSize) -> ViewerRect {
        guard hasArea, size.isUsable else { return self }
        let scale = min(width / size.width, height / size.height)
        guard scale.isFinite, scale > 0 else { return self }
        let w = size.width * scale, h = size.height * scale
        return ViewerRect(x: x + (width - w) / 2, y: y + (height - h) / 2, width: w, height: h)
    }

    /// The overlap with `other`, `nil` when they do not overlap with a positive area.
    public func intersection(_ other: ViewerRect) -> ViewerRect? {
        guard hasArea, other.hasArea else { return nil }
        let left = max(x, other.x), top = max(y, other.y)
        let right = min(maxX, other.maxX), bottom = min(maxY, other.maxY)
        guard right > left, bottom > top else { return nil }
        return ViewerRect(x: left, y: top, width: right - left, height: bottom - top)
    }

    enum CodingKeys: String, CodingKey { case x, y, width, height }
}

/// Maps screen points into a view of a given size: the screen is aspect-fitted and centred
/// (letterboxed on the long sides). The Live view draws everything through this, so the
/// geometry has one implementation and is tested here.
public struct ViewerMapping: Sendable, Equatable {
    /// The screen size used (`ViewerSize.usable` of the one given).
    public let screen: ViewerSize
    /// The view size given (negative or non-finite sides count as 0).
    public let view: ViewerSize
    /// View points per screen point (0 when the view has no area).
    public let scale: Double
    /// Where the screen's top-left corner lands in the view.
    public let origin: ViewerPoint

    public init(screen: ViewerSize, view: ViewerSize) {
        let s = screen.usable
        let vw = view.width.isFinite ? max(view.width, 0) : 0
        let vh = view.height.isFinite ? max(view.height, 0) : 0
        self.screen = s
        self.view = ViewerSize(width: vw, height: vh)
        let scale = min(vw / s.width, vh / s.height)
        self.scale = scale.isFinite ? max(scale, 0) : 0
        self.origin = ViewerPoint(x: (vw - s.width * self.scale) / 2, y: (vh - s.height * self.scale) / 2)
    }

    /// The area of the view the screen occupies.
    public var contentRect: ViewerRect {
        ViewerRect(x: origin.x, y: origin.y, width: screen.width * scale, height: screen.height * scale)
    }

    /// A screen point in view coordinates (clamped to the screen first, so a point outside
    /// it lands on its edge, never in the letterbox).
    public func toView(_ p: ViewerPoint) -> ViewerPoint {
        let c = screen.clamp(p)
        return ViewerPoint(x: origin.x + c.x * scale, y: origin.y + c.y * scale)
    }

    /// A screen rectangle in view coordinates, clipped to the screen; `nil` when nothing of
    /// it is on the screen.
    public func toView(_ r: ViewerRect) -> ViewerRect? {
        guard let clipped = r.intersection(ViewerRect(x: 0, y: 0, width: screen.width, height: screen.height)) else {
            return nil
        }
        return ViewerRect(x: origin.x + clipped.x * scale, y: origin.y + clipped.y * scale,
                          width: clipped.width * scale, height: clipped.height * scale)
    }

    /// The screen point under a view point, `nil` outside the content or when the view has
    /// no area.
    public func toScreen(_ v: ViewerPoint) -> ViewerPoint? {
        guard scale > 0, v.isFinite else { return nil }
        let p = ViewerPoint(x: (v.x - origin.x) / scale, y: (v.y - origin.y) / scale)
        guard p.x >= 0, p.y >= 0, p.x <= screen.width, p.y <= screen.height else { return nil }
        return p
    }
}
