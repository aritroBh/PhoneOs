import Foundation
import JarvisCore

/// Total conversions from `Double`: `Int(x)` / `Int64(x)` / `UInt32(x)` trap on NaN,
/// infinity and out-of-range values, and real accessibility trees do contain them (system
/// UI reports non-finite and enormous frames). Every Double → integer conversion in
/// JarvisKit and the UI-test runner goes through these.
extension Double {
    /// Non-finite → 0, otherwise rounded toward zero and clamped to `Int`'s range.
    public var clampedInt: Int {
        guard isFinite else { return 0 }
        if self >= 9.2e18 { return Int.max }
        if self <= -9.2e18 { return Int.min }
        return Int(self)
    }

    /// Non-finite → 0, otherwise rounded toward zero and clamped to `0...UInt32.max`.
    public var clampedUInt32: UInt32 {
        guard isFinite, self > 0 else { return 0 }
        if self >= Double(UInt32.max) { return UInt32.max }
        return UInt32(self)
    }

    /// Non-finite → 0, otherwise clamped to `-limit...limit`.
    public func finiteClamped(_ limit: Double) -> Double {
        guard isFinite else { return 0 }
        return Swift.min(Swift.max(self, -limit), limit)
    }

    /// Short text for dumps and messages: integers without a fraction, one decimal
    /// otherwise, `nan` / `inf` / `-inf` and `%g` for huge values. Never traps.
    public var compactText: String {
        guard isFinite else { return isNaN ? "nan" : (self > 0 ? "inf" : "-inf") }
        if Swift.abs(self) >= 1e15 { return String(format: "%g", self) }
        if rounded() == self { return String(Int64(self)) }
        return String(format: "%.1f", self)
    }
}

extension Rect {
    /// Largest coordinate kept in a sanitised frame (points; far beyond any screen).
    public static let coordinateLimit: Double = 100_000

    /// A frame the core and XCUITest can use: non-finite components become 0, everything is
    /// clamped to ±`coordinateLimit`, negative sizes become 0.
    public var sanitized: Rect {
        let limit = Rect.coordinateLimit
        return Rect(
            x: x.finiteClamped(limit),
            y: y.finiteClamped(limit),
            width: Swift.max(0, width.finiteClamped(limit)),
            height: Swift.max(0, height.finiteClamped(limit))
        )
    }

    /// Whether every component is finite.
    public var isFinite: Bool {
        x.isFinite && y.isFinite && width.isFinite && height.isFinite
    }
}
