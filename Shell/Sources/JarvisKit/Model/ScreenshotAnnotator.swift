import Foundation
import JarvisCore

/// Draws set-of-marks labels (`ModelRequest.marks`, frames in screen points; convert with
/// `CapturedScreenshot.scale`) onto a screenshot before a vision call. Drawing needs an
/// image library (CoreGraphics on Apple platforms), so the real annotator lives in the
/// app; the default leaves the image as is.
public protocol ScreenshotAnnotator: Sendable {
    /// Returns `screenshot` with the numbered marks drawn on it.
    func annotate(_ screenshot: CapturedScreenshot, marks: [Mark]) async throws -> CapturedScreenshot
}

/// Leaves screenshots unannotated.
public struct NoAnnotation: ScreenshotAnnotator {
    /// The pass-through annotator.
    public init() {}

    public func annotate(_ screenshot: CapturedScreenshot, marks: [Mark]) async throws -> CapturedScreenshot {
        screenshot
    }
}
