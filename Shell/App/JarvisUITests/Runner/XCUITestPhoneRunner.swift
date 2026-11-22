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
        /// Put both raw lock readings in every observation's notes and in
        /// `lockSignalHistory` (the lock tests), not only when they change.
        var noteLockSignalsEveryObservation: Bool

        init(
            knownBundleIds: [String] = XCUITestPhoneRunner.defaultKnownApps,
            settleAfterAction: Duration = .milliseconds(700),
            keyboardWait: TimeInterval = 3,
            launchWait: TimeInterval = 15,
            noteLockSignalsEveryObservation: Bool = false
        ) {
            self.knownBundleIds = knownBundleIds
            self.settleAfterAction = settleAfterAction
            self.keyboardWait = keyboardWait
            self.launchWait = launchWait
            self.noteLockSignalsEveryObservation = noteLockSignalsEveryObservation
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
    private var notifyToken: Int32?
    private var lastLockEvidence: String?
    private var pendingLockNotes: [String] = []
    private let started = Date()
    /// Lock readings with the time since the runner was made: every change, and every
    /// reading with `noteLockSignalsEveryObservation` (for failure messages).
    private(set) var lockSignalHistory: [String] = []

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

    nonisolated func lockState() async throws -> DeviceLockState {
        await lockStateOnMain()
    }

    nonisolated func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        if case .wait(let ms) = command {
            try await Task.sleep(for: .milliseconds(Int(min(ms, 30_000)))) // capped: no UI-test wait over 30 s
            return RunnerResult(ok: true, error: nil)
        }
        if case .typeText = command {
            // Give a keyboard that is still animating in time to appear (off the main actor).
            if !(await keyboardIsUp()) { try await Task.sleep(for: .milliseconds(500)) }
        }
        if await lockSignals().forCommands == .locked {
            await noteRefusal(command)
            return .deviceLocked("the device is locked; \(Transcript.describe(command)) was not performed")
        }
        return await act(command)
    }

    /// After a command: the notify state alone (cheap, and it flips about 0.3 s before the
    /// front app does). A command that "succeeded" while the phone locked under it is
    /// reported as locked (`RunnerResult.checkedAfterCommand`), so the core re-observes
    /// after the unlock instead of trusting it.
    private func lockedAfterCommand() -> DeviceLockState {
        switch notifyLockState() {
        case true?: .locked
        case false?: .unlocked
        case nil: .unknown
        }
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
        let snapshotMs = (Date().timeIntervalSince(start) * 1_000).clampedInt
        let rootFrame = raw.frame.sanitized
        var size = Size(width: rootFrame.width, height: rootFrame.height)
        if size.width <= 0 || size.height <= 0 {
            // Points from the screenshot (pixels / scale): UIScreen.main is deprecated.
            let image = XCUIScreen.main.screenshot().image
            size = Size(width: Double(image.size.width).finiteClamped(Rect.coordinateLimit),
                        height: Double(image.size.height).finiteClamped(Rect.coordinateLimit))
            notes.append("the application element has no frame; screen size taken from a screenshot (\(size.width.compactText)x\(size.height.compactText))")
        }
        let built = UiTreeBuilder.build(raw, screenSize: size)
        let signals = LockSignals(
            notify: notifyLockState(),
            lockScreen: bundleId == Self.springboard ? LockScreenSignature.matches(raw) : nil,
            front: bundleId == Self.springboard ? .springBoard : .app(bundleId)
        )
        record(signals, everyTime: configuration.noteLockSignalsEveryObservation)
        // The resume rule: a stale notify state must not keep an observation "locked" while
        // an app or the home screen is in front (commands are still refused on either signal).
        let locked = signals.forResume == .locked
        notes += pendingLockNotes
        pendingLockNotes = []
        if built.sanitizedFrames > 0 {
            notes.append("\(built.sanitizedFrames) element frame(s) were non-finite, huge or negative and were sanitised (raw values in the tree dump)")
        }
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
                pixelWidth: Double(image.size.width * image.scale).clampedUInt32,
                pixelHeight: Double(image.size.height * image.scale).clampedUInt32,
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
            deviceLocked: locked,
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

    // MARK: - Lock state (main actor)
    //
    // Two readings, combined by `LockSignals` (JarvisKit, Linux-tested):
    // 1. Darwin notify state of "com.apple.springboard.lockstate" (non-zero = locked),
    //    looked up with dlsym so nothing here depends on how the SDK exposes notify.h;
    //    whether the simulator's notifyd publishes it to the test runner is [unverified].
    // 2. SpringBoard in front showing the lock screen (`LockScreenSignature`, from the
    //    run-16 probe trees).
    // Commands are refused when either says locked; `lockState()` (unlock polls and the
    // lock watch) answers with the resume rule, so a stale reading cannot latch a wait.
    // Every change of the readings goes into the next observation's notes.

    private func lockStateOnMain() -> DeviceLockState {
        lockSignals().forResume
    }

    /// Reads both signals now.
    private func lockSignals() -> LockSignals {
        var notes: [String] = []
        let front = foregroundBundleId(notes: &notes)
        let notify = notifyLockState()
        let signals: LockSignals
        if front == Self.springboard {
            let lockScreen = (try? application(Self.springboard).snapshot()).map { LockScreenSignature.matches(SnapshotCopy.copy($0)) }
            signals = LockSignals(notify: notify, lockScreen: lockScreen, front: .springBoard)
        } else {
            signals = LockSignals(notify: notify, lockScreen: nil, front: .app(front))
        }
        record(signals, everyTime: false)
        return signals
    }

    private func record(_ signals: LockSignals, everyTime: Bool) {
        let evidence = signals.description
        let changed = evidence != lastLockEvidence
        lastLockEvidence = evidence
        guard changed || everyTime else { return }
        pendingLockNotes.append(evidence)
        let seconds = String(format: "%.1f", Date().timeIntervalSince(started))
        lockSignalHistory.append("t+\(seconds)s \(evidence)")
    }

    private func notifyLockState() -> Bool? {
        typealias Register = @convention(c) (UnsafePointer<CChar>?, UnsafeMutablePointer<Int32>?) -> UInt32
        typealias GetState = @convention(c) (Int32, UnsafeMutablePointer<UInt64>?) -> UInt32
        guard let handle = dlopen(nil, RTLD_NOW),
              let registerSymbol = dlsym(handle, "notify_register_check"),
              let getSymbol = dlsym(handle, "notify_get_state") else { return nil }
        let register = unsafeBitCast(registerSymbol, to: Register.self)
        let getState = unsafeBitCast(getSymbol, to: GetState.self)
        if notifyToken == nil {
            var token: Int32 = 0
            let status = "com.apple.springboard.lockstate".withCString { register($0, &token) }
            guard status == 0 else { return nil }
            notifyToken = token
        }
        guard let token = notifyToken else { return nil }
        var state: UInt64 = 0
        guard getState(token, &state) == 0 else { return nil }
        return state != 0
    }

    private func noteRefusal(_ command: RunnerCommand) {
        onCommand?(command, .deviceLocked())
    }

    // MARK: - Commands (main actor)

    private func act(_ command: RunnerCommand) -> RunnerResult {
        settlePending = true
        let result = execute(command).checkedAfterCommand(command, lockedAfter: lockedAfterCommand())
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
        // Never hand XCTest a non-finite or absurd offset (the core computes points from
        // sanitised frames, but this is the last line before the event synthesiser).
        let limit = Rect.coordinateLimit
        return app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            .withOffset(CGVector(dx: x.finiteClamped(limit), dy: y.finiteClamped(limit)))
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
