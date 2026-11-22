import Foundation
import JarvisCore

// Small lookups on the core's Live view records (`JarvisViewer.timeline()`), so the SwiftUI
// view reads them as it read the Swift timeline before. No logic: the core decides.

extension CoreViewerTimeline {
    /// Before the first update (and when the viewer could not be made).
    public static let empty = CoreViewerTimeline(
        taskId: "", steps: [], droppedSteps: 0, screens: [], currentScreenId: nil, activity: .idle,
        activityCaption: "Standing by", generation: 0
    )

    /// The screen summary with `id`.
    public func screen(id: UInt32?) -> CoreViewerScreenInfo? {
        guard let id else { return nil }
        return screens.first { $0.id == id }
    }

    /// The latest observed screen.
    public var currentScreen: CoreViewerScreenInfo? { screen(id: currentScreenId) }

    /// The step numbered `number` (numbers are consecutive, oldest first).
    public func step(number: UInt32?) -> CoreViewerStep? {
        guard let number, let first = steps.first?.number, number >= first else { return nil }
        let i = Int(number - first)
        if steps.indices.contains(i), steps[i].number == number { return steps[i] }
        return steps.first { $0.number == number }
    }
}

extension CoreViewerStepState {
    public var isFailed: Bool {
        if case .failed = self { return true }
        return false
    }
}
