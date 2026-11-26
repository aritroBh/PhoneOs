import Foundation
import JarvisCore
import Testing
@testable import JarvisKit

/// Review pass 30 (M1): one depth bound. The core reads 160 levels
/// (`jarvis_proto::MAX_TREE_DEPTH`) and reads a deeper tree as an incomplete screen; the wire
/// refuses a deeper tree. The shell's copy must never cut at or above that bound silently:
/// `UiTreeBuilder` stops deeper than the core reads.
@Suite("Depth bound: the shell's copy cuts deeper than the core reads (UiTreeBuilder)")
struct DepthBoundTests {
    private func chain(_ levels: Int) -> RawElement {
        var node = RawElement(elementType: XCUIElementTypeCode.button, label: "Deep",
                              frame: Rect(x: 16, y: 100, width: 200, height: 44))
        for _ in 0..<levels {
            node = RawElement(elementType: XCUIElementTypeCode.other, frame: Rect(x: 0, y: 0, width: 393, height: 852),
                              children: [node])
        }
        return RawElement(elementType: XCUIElementTypeCode.application, frame: Rect(x: 0, y: 0, width: 393, height: 852),
                          children: [node])
    }

    private func depth(_ n: UiNode) -> Int {
        var best = 0
        var stack: [(UiNode, Int)] = [(n, 1)]
        while let (e, d) = stack.popLast() {
            best = max(best, d)
            stack.append(contentsOf: e.children.map { ($0, d + 1) })
        }
        return best
    }

    @Test("the copy's bound is deeper than the core's, and every level up to the core's is kept")
    func bound() {
        #expect(UiTreeBuilder.maxDepth > 160)
        let at = UiTreeBuilder.build(chain(260), screenSize: nil).root
        #expect(depth(at) == 262, "every level kept")
        let past = UiTreeBuilder.build(chain(400), screenSize: nil).root
        // Cut, but deeper than the core reads: the core sees the cut (an incomplete screen).
        #expect(depth(past) > 160)
        #expect(depth(past) <= UiTreeBuilder.maxDepth + 1)
    }
}
