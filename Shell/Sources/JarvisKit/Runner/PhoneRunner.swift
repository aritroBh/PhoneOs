import Foundation
import JarvisCore
import Synchronization

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

    /// Performs one command planned on the app `expect` names (`Effect.act`'s `expect`;
    /// `nil` for commands that carry none). A runner that `verifiesFrontApp` compares
    /// `expect` with the app in front itself, next to the gesture, and answers
    /// `RunnerResult.stale` (nothing done) when another app is in front or the front app
    /// cannot be read. The default ignores `expect` and calls `perform(_:)`: for such
    /// runners `FrontAppGuard`'s check before the call is the only one, as before.
    func perform(_ command: RunnerCommand, expect: CommandExpect?) async throws -> RunnerResult

    /// `true` when this runner can always tell which app is in front and checks `expect`
    /// itself in `perform(_:expect:)` (Jarvis's own runner, the in-process XCUITest
    /// runner). For such a runner a `frontBundleId()` that throws means the runner failed,
    /// not "cannot tell": `FrontAppGuard` then refuses the command (the error surfaces;
    /// nothing is sent) instead of sending it unchecked. Default `false`.
    var verifiesFrontApp: Bool { get }

    /// Whether the phone is locked now. `SessionDriver` polls it while the session waits for
    /// an unlock and while it waits for the model or the owner (a lock then interrupts the
    /// wait). Runners that cannot tell return `.unknown` (the default); the driver then
    /// relies on `RunnerObservation.deviceLocked` and `RunnerFailure.deviceLocked`.
    func lockState() async throws -> DeviceLockState

    /// A screenshot for the Live view only ("Show real screen in Live view"): the screenshot
    /// call alone, no tree read. `SessionDriver` calls it best-effort after an observation
    /// the core asked no pixels for, with its own short deadline; every error is ignored and
    /// nothing of it reaches the core. Runners that cannot take one return `nil` (the
    /// default): the Live view shows the wireframe.
    func viewerScreenshot() async throws -> CapturedScreenshot?

    /// The bundle id of the app in front now, read cheaply (no tree): what the stale-screen
    /// guard compares a command's `expect` with before sending it (`FrontAppGuard`).
    /// Runners that cannot tell return `nil` (the default): the command is sent, and the
    /// driver notes once that the check could not be made.
    func frontBundleId() async throws -> String?
}

extension PhoneRunner {
    /// Default: `expect` is not checked by the runner (see `FrontAppGuard`).
    public func perform(_ command: RunnerCommand, expect: CommandExpect?) async throws -> RunnerResult {
        try await perform(command)
    }

    /// Default: the runner may not be able to tell which app is in front.
    public var verifiesFrontApp: Bool { false }

    /// Default: the runner cannot tell.
    public func lockState() async throws -> DeviceLockState { .unknown }

    /// Default: the runner cannot tell which app is in front.
    public func frontBundleId() async throws -> String? { nil }

    /// Default: no Live view screenshots.
    public func viewerScreenshot() async throws -> CapturedScreenshot? { nil }
}

/// The phone's lock state as a runner reports it.
public enum DeviceLockState: String, Sendable, Equatable {
    case locked
    case unlocked
    /// The runner has no way to tell.
    case unknown
}

extension RunnerResult {
    /// The runner refused because the phone is locked; nothing was carried out.
    public static func deviceLocked(_ message: String = "the device is locked") -> RunnerResult {
        RunnerResult(ok: false, error: message, failure: .deviceLocked)
    }

    /// The command was refused because `front`, not the app it expected, is in front
    /// (stale-screen guard, layer 2); nothing was carried out.
    public static func stale(front: String, expected: String) -> RunnerResult {
        RunnerResult(
            ok: false,
            error: "not carried out: \(front) is in front, not \(expected), the app the command was planned on",
            failure: .stale
        )
    }

    /// The command was refused because the app in front could not be read where the check
    /// has to be made (by a runner that checks `expect` itself); nothing was carried out.
    public static func staleUnreadable(expected: String) -> RunnerResult {
        RunnerResult(
            ok: false,
            error: "not carried out: the app in front could not be read, and the command was planned on \(expected)",
            failure: .stale
        )
    }
}

/// Stale-screen guard, layer 2 (docs/ARCHITECTURE.md, *Stale-screen guard*): a gesture or
/// typing command says which app it was planned on (`Effect.act`'s `expect`), and is
/// refused (`RunnerFailure.stale`, nothing sent) when the runner reads another app in front
/// (`PhoneRunner.frontBundleId()`), so a tap planned on one app never lands on whatever
/// replaced it. The core treats the refusal like a locked one: the step is refunded, it
/// observes again and decides again.
///
/// - Same app (bundle ids compared ignoring case and surrounding spaces): sent.
/// - Another app: refused.
/// - Unknown (the runner returns `nil` or the read fails): sent, and the first such command
///   of the run returns a note saying the check could not be made (once per guard).
/// - No `expect` (launching an app, opening a link, waiting, Home): sent unchecked.
/// - A runner that `verifiesFrontApp` (twenty-first review): a read that throws refuses the
///   command (the error is rethrown, nothing is sent), since for such a runner a throw is a
///   failure, not "cannot tell"; a `nil` reading sends it without a note. Either way the
///   command goes out with its `expect` (`perform(_:expect:)`) and the runner checks it
///   again next to the gesture, so a channel that fails between the two calls cannot make
///   an unchecked command.
///
/// `SessionDriver` runs every command through one guard, so every runner gets the check.
/// Limits: between the read and the synthesized touch a few milliseconds remain, and
/// overlays SpringBoard owns (banners, system alerts, the call screen) may not change the
/// front app.
public final class FrontAppGuard: Sendable {
    private let unknownNoted = Mutex(false)

    public init() {}

    /// The note given (once) when the front app cannot be read.
    public static let unknownNote =
        "Stale-screen check: this runner cannot tell which app is in front, so commands are sent without checking it."

    /// What `perform` did: the runner's result (or the refusal) and a note to show, if any.
    public struct Outcome: Sendable, Equatable {
        public var result: RunnerResult
        public var note: String?
    }

    /// Whether `expect` holds for the reading `front`: `true` (send), `false` (refuse), `nil`
    /// (unknown: send).
    public static func holds(_ expect: CommandExpect, front: String?) -> Bool? {
        guard let front else { return nil }
        let want = expect.bundleId.trimmingCharacters(in: .whitespaces)
        return want.caseInsensitiveCompare(front.trimmingCharacters(in: .whitespaces)) == .orderedSame
    }

    /// Checks `expect` against `runner`'s front app, then sends `command` (or refuses it).
    public func perform(_ command: RunnerCommand, expect: CommandExpect?, on runner: any PhoneRunner) async throws -> Outcome {
        guard let expect else {
            return Outcome(result: try await runner.perform(command, expect: nil), note: nil)
        }
        let front: String?
        if runner.verifiesFrontApp {
            // A throw here is the runner failing: refuse (surface it), never send unchecked.
            front = try await runner.frontBundleId()
        } else {
            front = try? await runner.frontBundleId()
        }
        switch Self.holds(expect, front: front) {
        case false?:
            return Outcome(result: .stale(front: front ?? "", expected: expect.bundleId), note: nil)
        case true?:
            return Outcome(result: try await runner.perform(command, expect: expect), note: nil)
        case nil:
            // The runner checks `expect` itself next to the gesture: nothing to note.
            if runner.verifiesFrontApp {
                return Outcome(result: try await runner.perform(command, expect: expect), note: nil)
            }
            let first = unknownNoted.withLock { noted in
                defer { noted = true }
                return !noted
            }
            return Outcome(result: try await runner.perform(command, expect: expect), note: first ? Self.unknownNote : nil)
        }
    }

    /// For a runner that checks `expect` itself (`verifiesFrontApp`), in the same step as the
    /// gesture: the refusal to answer instead of acting when `front` (the reading taken right
    /// there) is another app or could not be read (`nil`); `nil` when the command may run
    /// (it carries no `expect`, or the app it was planned on is in front).
    public static func refusalNextToGesture(_ expect: CommandExpect?, front: String?) -> RunnerResult? {
        guard let expect else { return nil }
        switch holds(expect, front: front) {
        case true?: return nil
        case false?: return .stale(front: front ?? "", expected: expect.bundleId)
        case nil: return .staleUnreadable(expected: expect.bundleId)
        }
    }
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
    /// The runner reports the phone locked: the tree is of the lock screen and the core
    /// ignores it and waits for an unlock.
    public var deviceLocked: Bool
    /// The lock state the runner read for this observation: `.unlocked` only when the
    /// runner positively reported the phone unlocked (LoopCtrl's `device_info`), `.unknown`
    /// when it cannot tell (the default). The Live view takes real-screen pixels only for a
    /// known-unlocked observation (`isKnownUnlocked`).
    public var lockState: DeviceLockState
    /// Things the runner adapter had to assume or default (shown in the driver's updates,
    /// so nothing is guessed silently).
    public var notes: [String]

    /// Not locked, and the runner said so.
    public var isKnownUnlocked: Bool { !deviceLocked && lockState == .unlocked }

    /// An observation carrying the runner's text tree.
    public init(
        source: String,
        appBundleId: String,
        appName: String? = nil,
        screenSize: Size,
        keyboardVisible: Bool = false,
        orientation: Orientation = .portrait,
        screenshot: CapturedScreenshot? = nil,
        deviceLocked: Bool = false,
        notes: [String] = [],
        lockState: DeviceLockState? = nil
    ) {
        self.init(
            ui: .runnerText(source), appBundleId: appBundleId, appName: appName, screenSize: screenSize,
            keyboardVisible: keyboardVisible, orientation: orientation, screenshot: screenshot,
            deviceLocked: deviceLocked, notes: notes, lockState: lockState
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
        deviceLocked: Bool = false,
        notes: [String] = [],
        lockState: DeviceLockState? = nil
    ) {
        self.init(
            ui: .tree(root), appBundleId: appBundleId, appName: appName, screenSize: screenSize,
            keyboardVisible: keyboardVisible, orientation: orientation, screenshot: screenshot,
            deviceLocked: deviceLocked, notes: notes, lockState: lockState
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
        deviceLocked: Bool = false,
        notes: [String],
        lockState: DeviceLockState? = nil
    ) {
        self.lockState = lockState ?? (deviceLocked ? .locked : .unknown)
        self.ui = ui
        self.appBundleId = appBundleId
        self.appName = appName
        self.screenSize = screenSize
        self.keyboardVisible = keyboardVisible
        self.orientation = orientation
        self.screenshot = screenshot
        self.deviceLocked = deviceLocked
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
                screenshot: ref,
                deviceLocked: deviceLocked
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
                screenshot: ref,
                deviceLocked: deviceLocked
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

extension RunnerResult {
    /// The result to report for `command` when the lock reading right after it is
    /// `lockedAfter`. A command that "succeeded" while the phone locked under it most
    /// likely landed on the lock screen and did nothing, so it is reported as
    /// `RunnerFailure.deviceLocked`: the core does not trust it and looks again after the
    /// unlock. Waits and results already marked locked are kept; `.unknown` changes nothing.
    public func checkedAfterCommand(_ command: RunnerCommand, lockedAfter: DeviceLockState) -> RunnerResult {
        guard lockedAfter == .locked, failure != .deviceLocked else { return self }
        if case .wait = command { return self }
        return .deviceLocked("the phone locked as \(Transcript.describe(command)) ran; it may not have taken effect")
    }
}

/// Wraps a runner that can read the lock state cheaply (LoopCtrl's `device_info`) and checks
/// it before and after every command.
///
/// - **Before**: model inference sits between the observation and the command and can take
///   many seconds, so the phone may have locked meanwhile. When the runner reports
///   `.locked`, the command is not sent and the result is the runner's own locked refusal
///   (`RunnerResult.deviceLocked`, `RunnerFailure.deviceLocked`: nothing ran), so the core
///   waits for the unlock, observes again and chooses the action again. `.unknown` or a
///   failed read sends the command as before.
/// - **After**: `RunnerResult.checkedAfterCommand` (a command the phone locked under is
///   reported as locked). A failed lock read changes nothing.
///
/// Waits (`.wait`) are not checked. Observations and lock polls pass through.
public final class LockCheckingRunner: PhoneRunner {
    public let inner: any PhoneRunner

    public init(_ inner: any PhoneRunner) {
        self.inner = inner
    }

    public func snapshot(includeScreenshot: Bool) async throws -> RunnerObservation {
        try await inner.snapshot(includeScreenshot: includeScreenshot)
    }

    public func perform(_ command: RunnerCommand) async throws -> RunnerResult {
        try await perform(command, expect: nil)
    }

    public func perform(_ command: RunnerCommand, expect: CommandExpect?) async throws -> RunnerResult {
        if case .wait = command { return try await inner.perform(command, expect: expect) }
        if (try? await inner.lockState()) == .locked {
            return .deviceLocked("the phone is locked; \(Transcript.describe(command)) was not sent")
        }
        let result = try await inner.perform(command, expect: expect)
        let after = (try? await inner.lockState()) ?? .unknown
        return result.checkedAfterCommand(command, lockedAfter: after)
    }

    public var verifiesFrontApp: Bool { inner.verifiesFrontApp }

    public func lockState() async throws -> DeviceLockState {
        try await inner.lockState()
    }

    public func viewerScreenshot() async throws -> CapturedScreenshot? {
        try await inner.viewerScreenshot()
    }

    public func frontBundleId() async throws -> String? {
        try await inner.frontBundleId()
    }
}
