import Foundation
import JarvisCore

// The one piece of Live view geometry the shell does itself: placing the wireframe's
// rectangles, which the core sends in screen points (`JarvisViewer.screen(id:)`), with the
// mapping of the frame's scene. Everything else the view draws comes in view points.
// Platform-free (no CoreGraphics) and total: NaN, infinity and empty rectangles give `nil`.

extension CoreLiveScene {
    /// `frame` (screen points) in view points, clipped to the screen first; `nil` when
    /// nothing of it is on the screen.
    public func viewRect(_ frame: Rect) -> Rect? {
        let w = screenSize.width, h = screenSize.height
        let values = [frame.x, frame.y, frame.width, frame.height, w, h, scale, origin.x, origin.y]
        guard values.allSatisfy(\.isFinite), frame.width > 0, frame.height > 0 else { return nil }
        let left = max(frame.x, 0), top = max(frame.y, 0)
        let right = min(frame.x + frame.width, w), bottom = min(frame.y + frame.height, h)
        guard right > left, bottom > top else { return nil }
        return Rect(x: origin.x + left * scale, y: origin.y + top * scale,
                    width: (right - left) * scale, height: (bottom - top) * scale)
    }
}
