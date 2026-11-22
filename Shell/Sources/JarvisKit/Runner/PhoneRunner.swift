import Foundation
import JarvisCore

/// The actuator: whatever drives the phone's UI (the XCUITest runner on the device, a
/// simulator, a fake in tests). `SessionDriver` calls one method at a time.
public protocol PhoneRunner: Sendable {
    /// Reads the foreground app's UI (the runner's text tree or a typed tree) plus what the
    /// core needs beside it, and a screenshot when `includeScreenshot` is true and the runner can
    /// take one. Throws when the UI could not be read.
    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation

    /// Performs one command. Returns `RunnerResult(ok: false, …)` when the runner answered
    /// that it could not do it; throws when the runner could not be reached or its answer
    /// could not be read.
    func perform(_ command: RunnerCommand) async throws -> RunnerResult
}

/// The UI part of an observation: either the runner's text tree (parsed by the core) or a
/// typed tree the runner built itself.
public enum ObservedUI: Sendable, Equatable {
    /// The runner's raw text tree (LoopCtrl `page_source` format); the core parses it and
    /// assumes `enabled`, `visible`, `hittable` true and `focused`, `selected` false.
    case runnerText(String)
    /// A typed tree (root is normally the `Application` node). Keeps `enabled`,
    /// `focused`, `selected` and `secure`, which the text format cannot carry.
    case tree(UiNode)
}

/// One observation of the phone, before the driver hands it to the core.
public struct RunnerObservation: Sendable, Equatable {
    /// The UI tree, as text or typed.
    public var ui: ObservedUI
    /// Bundle id of the foreground app (the deny list is checked against it).
    public var appBundleId: String
    /// Display name of the foreground app, when known.
    public var appName: String?
    /// Screen size in points.
    public var screenSize: Size
    /// Whether the software keyboard is up.
    public var keyboardVisible: Bool
    /// Interface orientation.
    public var orientation: Orientation
    /// The screenshot, when one was requested and taken.
    public var screenshot: CapturedScreenshot?
    /// Things the runner adapter had to assume or default (shown in the driver's updates,
    /// so nothing is guessed silently).
    public var notes: [String]

    /// An observation carrying the runner's text tree.
    public init(
        source: String,
        appBundleId: String,
        appName: String? = nil,
        screenSize: Size,
        keyboardVisible: Bool = false,
        orientation: Orientation = .portrait,
        screenshot: CapturedScreenshot? = nil,
        notes: [String] = []
    ) {
        self.init(
            ui: .runnerText(source), appBundleId: appBundleId, appName: appName, screenSize: screenSize,
            keyboardVisible: keyboardVisible, orientation: orientation, screenshot: screenshot, notes: notes
        )
    }

    /// An observation carrying a typed tree.
    public init(
        root: UiNode,
        appBundleId: String,
        appName: String? = nil,
        screenSize: Size,
        keyboardVisible: Bool = false,
        orientation: Orientation = .portrait,
        screenshot: CapturedScreenshot? = nil,
        notes: [String] = []
    ) {
        self.init(
            ui: .tree(root), appBundleId: appBundleId, appName: appName, screenSize: screenSize,
            keyboardVisible: keyboardVisible, orientation: orientation, screenshot: screenshot, notes: notes
        )
    }

    /// An observation from its parts.
    public init(
        ui: ObservedUI,
        appBundleId: String,
        appName: String?,
        screenSize: Size,
        keyboardVisible: Bool,
        orientation: Orientation,
        screenshot: CapturedScreenshot?,
        notes: [String]
    ) {
        self.ui = ui
        self.appBundleId = appBundleId
        self.appName = appName
        self.screenSize = screenSize
        self.keyboardVisible = keyboardVisible
        self.orientation = orientation
        self.screenshot = screenshot
        self.notes = notes
    }

    /// The runner text, when this observation carries text.
    public var source: String? {
        if case .runnerText(let text) = ui { return text }
        return nil
    }

    /// The typed root, when this observation carries a typed tree.
    public var root: UiNode? {
        if case .tree(let root) = ui { return root }
        return nil
    }

    /// The core event for this observation, with the stored screenshot's handle:
    /// `Event.observedRunnerText` for text, `Event.observed` for a typed tree. The one
    /// place a `RunnerSnapshot` / `Screen` is built from an observation, so a regenerated
    /// binding with more fields needs one edit.
    func event(screenshot ref: ImageRef?) -> Event {
        switch ui {
        case .runnerText(let source):
            return .observedRunnerText(snapshot: RunnerSnapshot(
                source: source,
                appBundleId: appBundleId,
                appName: appName,
                size: screenSize,
                keyboardVisible: keyboardVisible,
                orientation: orientation,
                screenshot: ref
            ))
        case .tree(let root):
            return .observed(observation: ScreenObservation(
                screen: Screen(
                    appBundleId: appBundleId,
                    appName: appName,
                    size: screenSize,
                    root: root,
                    keyboardVisible: keyboardVisible,
                    orientation: orientation
                ),
                screenshot: ref
            ))
        }
    }
}

/// Encoded screenshot bytes as the runner returned them. Pixels never go to the core: the
/// driver keeps them in a `ScreenshotStore` and passes an `ImageRef`.
public struct CapturedScreenshot: Sendable, Equatable {
    /// The encoded image (JPEG or PNG).
    public var data: Data
    /// MIME type of `data`, e.g. `image/jpeg`.
    public var mimeType: String
    /// Width in pixels.
    public var pixelWidth: UInt32
    /// Height in pixels.
    public var pixelHeight: UInt32
    /// Pixels per point, when the runner reported it.
    public var scale: Double?

    /// Creates a screenshot value.
    public init(data: Data, mimeType: String, pixelWidth: UInt32, pixelHeight: UInt32, scale: Double? = nil) {
        self.data = data
        self.mimeType = mimeType
        self.pixelWidth = pixelWidth
        self.pixelHeight = pixelHeight
        self.scale = scale
    }
}
