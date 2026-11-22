import Foundation
import Synchronization
import UIKit
import XCTest
import JarvisCore
import JarvisKit

/// One observation as the runner saw it, for QA artifacts.
struct RunnerObservationRecord {
    /// 1-based observation number in this runner's life.
    var number: Int
    var bundleId: String
    var raw: RawElement
    var built: UiTreeBuilder.Result
    var screenshot: XCUIScreenshot?
    var notes: [String]
    /// Milliseconds the XCUITest snapshot took.
    var snapshotMs: Int
}

/// `PhoneRunner` over public XCUITest API, running inside the UI-test process (the
/// simulator QA harness; on a device the LoopCtrl runner plays this part).
///
/// ## Threading
/// XCUITest calls are synchronous and must run on the main thread: they block while
/// XCTest talks to the accessibility server and waits for the app to go idle. The class is
/// `@MainActor`; the two `PhoneRunner` requirements are `nonisolated async` and hop to the
/// main actor for the XCUITest part only. The `SessionDriver` actor awaits them, so its
/// executor is never blocked. This works because the test method that awaits
/// `SessionDriver.run()` is itself `@MainActor async`: while it is suspended, the main
/// actor is free to run these hops. Pauses (settle time, `wait`) are `Task.sleep` outside
/// the main actor, never a blocking sleep on it.
@MainActor
final class XCUITestPhoneRunner: PhoneRunner {
    struct Configuration: Sendable {
        /// Apps checked (in order) when the last activated app is no longer in front.
        var knownBundleIds: [String]
        /// Pause after a command before the next snapshot (animations, keyboard).
        var settleAfterAction: Duration
        /// Longest wait for the software keyboard before typing.
        var keyboardWait: TimeInterval
        /// Longest wait for a launched app to reach the foreground.
        var launchWait: TimeInterval

        init(
            knownBundleIds: [String] = XCUITestPhoneRunner.defaultKnownApps,
            settleAfterAction: Duration = .milliseconds(700),
            keyboardWait: TimeInterval = 3,
            launchWait: TimeInterval = 15
        ) {
            self.knownBundleIds = knownBundleIds
            self.settleAfterAction = settleAfterAction
            self.keyboardWait = keyboardWait
            self.launchWait = launchWait
        }
    }

    static let springboard = "com.apple.springboard"
    nonisolated static let defaultKnownApps = [
        "dev.jarvis.app", "com.apple.Preferences", "com.apple.mobilesafari", "com.apple.MobileSMS",
        "com.apple.mobilenotes", "com.apple.mobilecal", "com.apple.reminders", "com.apple.mobileslideshow",
        "com.apple.Maps", "com.apple.DocumentsApp", "com.apple.MobileAddressBook", "com.apple.Health",
    ]

    nonisolated let configuration: Configuration
    /// Observations taken so far (readable from any thread: the QA harness ties prompts to
    /// the observation they were built from).
    nonisolated let observationCounter = Mutex(0)
    /// Called on the main actor after every observation.
    var onObservation: ((RunnerObservationRecord) -> Void)?
    /// Called on the main actor after every command with its result.
    var onCommand: ((RunnerCommand, RunnerResult) -> Void)?

    private var lastActivated: String?
    private var applications: [String: XCUIApplication] = [:]
    private var settlePending = false
    private var focusNoteGiven = false

    init(configuration: Configuration = Configuration()) {
        self.configuration = configuration
    }

    /// Tells the runner which app the test launched itself.
    func noteActivated(_ bundleId: String) {
        lastActivated = bundleId
    }

    // MARK: - PhoneRunner

    nonisolated func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        if await takeSettle() {
            try await Task.sleep(for: configuration.settleAfterAction)
        }
        return try await observe(includeScreenshot: includeScreenshot)
    }

    nonisolated func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        if case .wait(let ms) = command {
            try await Task.sleep(for: .milliseconds(Int(min(ms, 60_000))))
            return RunnerResult(ok: true, error: nil)
        }
        if case .typeText = command {
            // Give a keyboard that is still animating in time to appear (off the main actor).
            if !(await keyboardIsUp()) { try await Task.sleep(for: .milliseconds(500)) }
        }
        return await act(command)
    }

    // MARK: - Observation (main actor)

    private func takeSettle() -> Bool {
        defer { settlePending = false }
        return settlePending
    }

    private func observe(includeScreenshot: Bool) throws -> RunnerObservation {
        var notes: [String] = []
        let bundleId = foregroundBundleId(notes: &notes)
        let app = application(bundleId)
        let start = Date()
        let snapshot = try app.snapshot()
        let raw = SnapshotCopy.copy(snapshot)
        let snapshotMs = Int(Date().timeIntervalSince(start) * 1_000)
        var size = Size(width: raw.frame.width, height: raw.frame.height)
        if size.width <= 0 || size.height <= 0 {
            let screen = UIScreen.main.bounds.size
            size = Size(width: Double(screen.width), height: Double(screen.height))
            notes.append("the application element has no frame; screen size taken from UIScreen (\(size.width)x\(size.height))")
        }
        let built = UiTreeBuilder.build(raw, screenSize: size)
        if raw.subtreeFocus == nil, !focusNoteGiven {
            focusNoteGiven = true
            notes.append("keyboard focus is not readable from snapshots on this iOS; focused flags are false")
        }
        let shot = XCUIScreen.main.screenshot()
        var captured: CapturedScreenshot?
        if includeScreenshot {
            let image = shot.image
            captured = CapturedScreenshot(
                data: shot.pngRepresentation,
                mimeType: "image/png",
                pixelWidth: UInt32(max(0, image.size.width * image.scale)),
                pixelHeight: UInt32(max(0, image.size.height * image.scale)),
                scale: Double(image.scale)
            )
        }
        let number = observationCounter.withLock { n -> Int in
            n += 1
            return n
        }
        onObservation?(RunnerObservationRecord(
            number: number, bundleId: bundleId, raw: raw, built: built, screenshot: shot,
            notes: notes, snapshotMs: snapshotMs
        ))
        return RunnerObservation(
            root: built.root,
            appBundleId: bundleId,
            appName: raw.label.isEmpty ? nil : raw.label,
            screenSize: size,
            keyboardVisible: built.keyboardVisible,
            orientation: orientation(),
            screenshot: captured,
            notes: notes
        )
    }

    /// The app in front: the last activated one while it is in front, else the first known
    /// app in front, else SpringBoard (which counts as "in front" under every app, so it is
    /// checked last).
    private func foregroundBundleId(notes: inout [String]) -> String {
        if let id = lastActivated, application(id).state == .runningForeground {
            return id
        }
        for id in configuration.knownBundleIds where id != Self.springboard && id != lastActivated {
            if application(id).state == .runningForeground {
                if lastActivated != nil { notes.append("foreground changed to \(id)") }
                lastActivated = id
                return id
            }
        }
        if let id = lastActivated {
            notes.append("\(id) is no longer in front (state \(application(id).state.rawValue)); reading SpringBoard")
        }
        lastActivated = nil
        return Self.springboard
    }

    private func application(_ bundleId: String) -> XCUIApplication {
        if let app = applications[bundleId] { return app }
        let app = XCUIApplication(bundleIdentifier: bundleId)
        applications[bundleId] = app
        return app
    }

    private func orientation() -> Orientation {
        switch XCUIDevice.shared.orientation {
        case .portraitUpsideDown: return .portraitUpsideDown
        case .landscapeLeft: return .landscapeLeft
        case .landscapeRight: return .landscapeRight
        default: return .portrait
        }
    }

    private func keyboardIsUp() -> Bool {
        var notes: [String] = []
        return application(foregroundBundleId(notes: &notes)).keyboards.count > 0
    }

    // MARK: - Commands (main actor)

    private func act(_ command: RunnerCommand) -> RunnerResult {
        settlePending = true
        let result = execute(command)
        onCommand?(command, result)
        return result
    }

    private func execute(_ command: RunnerCommand) -> RunnerResult {
        var notes: [String] = []
        let bundleId = foregroundBundleId(notes: &notes)
        let app = application(bundleId)
        switch command {
        case .tap(let x, let y):
            point(app, x, y).tap()
            return ok
        case .longPress(let x, let y, let ms):
            point(app, x, y).press(forDuration: Double(ms) / 1_000)
            return ok
        case .swipe(let from, let to, _):
            // press(forDuration:thenDragTo:) drags at XCTest's own speed; the requested
            // duration is not honoured (a short hold keeps it a drag, not a long press).
            point(app, from.x, from.y).press(forDuration: 0.05, thenDragTo: point(app, to.x, to.y))
            return ok
        case .typeText(let text):
            if let refusal = typingRefusal(app) { return refusal }
            app.typeText(text)
            return ok
        case .pressKey(let key):
            return press(key, in: app)
        case .launchApp(let id):
            let target = application(id)
            target.activate()
            lastActivated = id
            guard target.wait(for: .runningForeground, timeout: configuration.launchWait) else {
                return fail("\(id) did not reach the foreground within \(configuration.launchWait) s (state \(target.state.rawValue))")
            }
            return ok
        case .openUrl(let text):
            guard let url = URL(string: text) else { return fail("not a URL: \(text)") }
            XCUIDevice.shared.system.open(url)
            // The handling app is found on the next observation.
            lastActivated = nil
            return ok
        case .wait:
            return ok // handled before the main-actor hop
        }
    }

    private func press(_ key: Key, in app: XCUIApplication) -> RunnerResult {
        switch key {
        case .home:
            XCUIDevice.shared.press(.home)
            lastActivated = nil
            return ok
        case .`return`:
            return typeKey("\n", app)
        case .delete:
            return typeKey(XCUIKeyboardKey.delete.rawValue, app)
        case .tab:
            return typeKey("\t", app)
        case .space:
            return typeKey(" ", app)
        case .escape:
            return typeKey(XCUIKeyboardKey.escape.rawValue, app)
        }
    }

    private func typeKey(_ text: String, _ app: XCUIApplication) -> RunnerResult {
        if let refusal = typingRefusal(app) { return refusal }
        app.typeText(text)
        return ok
    }

    /// `typeText` without keyboard focus is an XCTest failure, not an error, so check first:
    /// refuse when there is neither a keyboard nor a focused element. When focus cannot be
    /// read at all, typing is attempted (a hardware-keyboard simulator shows no keyboard).
    private func typingRefusal(_ app: XCUIApplication) -> RunnerResult? {
        if app.keyboards.firstMatch.waitForExistence(timeout: configuration.keyboardWait) { return nil }
        guard let snapshot = try? app.snapshot() else { return nil }
        let raw = SnapshotCopy.copy(snapshot)
        switch raw.subtreeFocus {
        case true?, nil:
            return nil
        case false?:
            return fail("no software keyboard and no focused element in \(app.identifierDescription); tap a text field first")
        }
    }

    private func point(_ app: XCUIApplication, _ x: Double, _ y: Double) -> XCUICoordinate {
        // The core sends absolute screen points; the app's frame origin is the screen origin
        // in portrait, so offset from the normalised (0, 0) corner.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0)).withOffset(CGVector(dx: x, dy: y))
    }

    private var ok: RunnerResult { RunnerResult(ok: true, error: nil) }

    private func fail(_ why: String) -> RunnerResult { RunnerResult(ok: false, error: why) }
}

private extension XCUIApplication {
    /// A short description for messages (XCUIApplication has no public bundle id).
    var identifierDescription: String {
        let label = (try? snapshot().label) ?? ""
        return label.isEmpty ? "the foreground app" : "\"\(label)\""
    }
}
