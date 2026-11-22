import Foundation
import JarvisCore

/// The actuator: whatever drives the phone's UI (the XCUITest runner on the device, a
/// simulator, a fake in tests). `SessionDriver` calls one method at a time.
public protocol PhoneRunner: Sendable {
    /// Reads the foreground app's UI as the runner's text tree plus what the core needs
    /// beside it, and a screenshot when `includeScreenshot` is true and the runner can
    /// take one. Throws when the UI could not be read.
    func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation

    /// Performs one command. Returns `RunnerResult(ok: false, …)` when the runner answered
    /// that it could not do it; throws when the runner could not be reached or its answer
    /// could not be read.
    func perform(_ command: RunnerCommand) async throws -> RunnerResult
}

/// One observation of the phone, before the driver hands it to the core.
public struct RunnerObservation: Sendable, Equatable {
    /// The runner's raw text tree (LoopCtrl `page_source` format); the core parses it.
    public var source: String
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

    /// Creates an observation.
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
        self.source = source
        self.appBundleId = appBundleId
        self.appName = appName
        self.screenSize = screenSize
        self.keyboardVisible = keyboardVisible
        self.orientation = orientation
        self.screenshot = screenshot
        self.notes = notes
    }

    /// The FFI snapshot for `Event.observedRunnerText`, with the stored screenshot's handle.
    /// The one place a `RunnerSnapshot` is built, so a regenerated binding with more fields
    /// needs one edit.
    func runnerSnapshot(screenshot ref: ImageRef?) -> RunnerSnapshot {
        RunnerSnapshot(
            source: source,
            appBundleId: appBundleId,
            appName: appName,
            size: screenSize,
            keyboardVisible: keyboardVisible,
            orientation: orientation,
            screenshot: ref
        )
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
